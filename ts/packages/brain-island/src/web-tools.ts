import { lookup } from "node:dns/promises";
import { isIP } from "node:net";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import { Type, type Static } from "typebox";
import type { BrainToolResolver } from "./tool-session-selection.js";

export interface WebSearchResult {
  title: string;
  url: string;
  snippet: string;
}

export interface WebSearchProvider {
  readonly name: string;
  search(
    query: string,
    maxResults: number,
    signal?: AbortSignal,
  ): Promise<readonly WebSearchResult[]>;
}

export interface ResolvedAddress {
  address: string;
  family: 4 | 6;
}

export type ResolveHostAddresses = (
  hostname: string,
) => Promise<readonly ResolvedAddress[]>;

export interface WebToolContext {
  provider?: WebSearchProvider;
  fetchImpl?: typeof fetch;
  env?: Readonly<Record<string, string | undefined>>;
  searxngUrl?: string;
  searchDefaultLimit?: number;
  searchMaxResults?: number;
  maxExtractUrls?: number;
  maxExtractChars?: number;
  maxExtractBytes?: number;
  maxRedirects?: number;
  allowPrivateNet?: boolean;
  allowedNonstandardPorts?: readonly number[];
  resolveHostAddresses?: ResolveHostAddresses;
}

export type WebSearchToolContext = WebToolContext;
export type WebExtractToolContext = WebToolContext;

export interface WebSearchToolDetails {
  ok: boolean;
  provider: string;
  query: string;
  maxResults: number;
  results: readonly WebSearchResult[];
  error?: string;
}

export interface WebExtractResult {
  url: string;
  finalUrl?: string;
  title: string;
  content: string;
  status?: number;
  contentType?: string;
  redirectCount: number;
  truncated: boolean;
  error?: string;
  reasonCode?: string;
}

export interface WebExtractToolDetails {
  ok: boolean;
  maxUrls: number;
  maxExtractChars: number;
  maxExtractBytes: number;
  maxRedirects: number;
  allowPrivateNet: boolean;
  results: readonly WebExtractResult[];
}

export interface WebNetworkPolicy {
  allowPrivateNet: boolean;
  allowedNonstandardPorts: ReadonlySet<number>;
  resolveHostAddresses: ResolveHostAddresses;
}

const defaultSearchLimit = 5;
const defaultSearchMaxResults = 10;
const defaultMaxExtractUrls = 5;
const defaultMaxExtractChars = 24_000;
const defaultMaxExtractBytes = 512 * 1024;
const defaultMaxRedirects = 5;
const userAgent = "rusty-crew-web-tool/0.1";

const webSearchParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  max_results: Type.Optional(
    Type.Number({ minimum: 1, maximum: defaultSearchMaxResults }),
  ),
});

const webExtractParameters = Type.Object({
  urls: Type.Array(Type.String({ minLength: 1 }), {
    minItems: 1,
    maxItems: defaultMaxExtractUrls,
  }),
});

type WebSearchParams = Static<typeof webSearchParameters>;
type WebExtractParams = Static<typeof webExtractParameters>;

export function createWebToolResolver(
  context: WebToolContext = {},
): BrainToolResolver {
  return () => resolveWebTools(context);
}

export function resolveWebTools(context: WebToolContext = {}): BrainTool[] {
  return [webSearchTool(context), webExtractTool(context)];
}

export function webSearchTool(
  context: WebSearchToolContext = {},
): BrainTool<typeof webSearchParameters, WebSearchToolDetails> {
  const searchDefaultLimit = clampInt(
    context.searchDefaultLimit ?? defaultSearchLimit,
    1,
    context.searchMaxResults ?? defaultSearchMaxResults,
  );
  const searchMaxResults = clampInt(
    context.searchMaxResults ?? defaultSearchMaxResults,
    1,
    defaultSearchMaxResults,
  );
  const provider = context.provider ?? createWebSearchProvider(context);
  return {
    name: "web_search",
    label: "Web search",
    description:
      "Search the public web through the configured provider and return bounded title/url/snippet results.",
    parameters: webSearchParameters,
    execute: async (_toolCallId, params: WebSearchParams, signal) => {
      const query = params.query.trim();
      const maxResults = clampInt(
        params.max_results ?? searchDefaultLimit,
        1,
        searchMaxResults,
      );
      try {
        const results = normalizeResults(
          await provider.search(query, maxResults, signal),
        ).slice(0, maxResults);
        return resultDetails({
          ok: true,
          provider: provider.name,
          query,
          maxResults,
          results,
        });
      } catch (error) {
        return resultDetails({
          ok: false,
          provider: provider.name,
          query,
          maxResults,
          results: [],
          error: errorMessage(error),
        });
      }
    },
  };
}

export function webExtractTool(
  context: WebExtractToolContext = {},
): BrainTool<typeof webExtractParameters, WebExtractToolDetails> {
  const fetchImpl = context.fetchImpl ?? fetch;
  const maxUrls = clampInt(
    context.maxExtractUrls ?? defaultMaxExtractUrls,
    1,
    defaultMaxExtractUrls,
  );
  const maxExtractChars = clampInt(
    context.maxExtractChars ?? defaultMaxExtractChars,
    1,
    defaultMaxExtractChars,
  );
  const maxExtractBytes = clampInt(
    context.maxExtractBytes ?? defaultMaxExtractBytes,
    1024,
    defaultMaxExtractBytes,
  );
  const maxRedirects = clampInt(
    context.maxRedirects ?? defaultMaxRedirects,
    0,
    defaultMaxRedirects,
  );
  const policy = webNetworkPolicy(context);
  return {
    name: "web_extract",
    label: "Web extract",
    description:
      "Fetch public HTTP(S) pages and return bounded text. Blocks localhost/private-network targets unless runtime policy allows them.",
    parameters: webExtractParameters,
    execute: async (_toolCallId, params: WebExtractParams, signal) => {
      const urls = params.urls.slice(0, maxUrls);
      const results = await Promise.all(
        urls.map((url) =>
          extractUrl(fetchImpl, url, {
            policy,
            maxExtractChars,
            maxExtractBytes,
            maxRedirects,
            signal,
          }),
        ),
      );
      return extractResultDetails({
        ok: results.every((result) => !result.error),
        maxUrls,
        maxExtractChars,
        maxExtractBytes,
        maxRedirects,
        allowPrivateNet: policy.allowPrivateNet,
        results,
      });
    },
  };
}

export function createWebSearchProvider(
  context: WebSearchToolContext = {},
): WebSearchProvider {
  const fetchImpl = context.fetchImpl ?? fetch;
  const searxngUrl = configuredSearxngUrl(context);
  if (searxngUrl) {
    return new SearxngProvider(fetchImpl, searxngUrl);
  }
  return new DuckDuckGoHtmlProvider(fetchImpl);
}

class SearxngProvider implements WebSearchProvider {
  readonly name = "searxng";

  constructor(
    private readonly fetchImpl: typeof fetch,
    private readonly baseUrl: string,
  ) {}

  async search(
    query: string,
    maxResults: number,
    signal?: AbortSignal,
  ): Promise<readonly WebSearchResult[]> {
    const url = new URL(this.baseUrl);
    url.searchParams.set("q", query);
    url.searchParams.set("format", "json");
    const response = await this.fetchImpl(url, {
      headers: { "user-agent": userAgent },
      signal,
    });
    if (!response.ok) {
      throw new Error(`SearXNG search failed with HTTP ${response.status}`);
    }
    const data = (await response.json()) as {
      readonly results?: readonly SearxngResult[];
    };
    return normalizeResults(
      (data.results ?? []).map((entry) => ({
        title: entry.title ?? entry.url ?? "untitled",
        url: entry.url ?? "",
        snippet: entry.content ?? "",
      })),
    ).slice(0, maxResults);
  }
}

interface SearxngResult {
  readonly title?: string;
  readonly url?: string;
  readonly content?: string;
}

class DuckDuckGoHtmlProvider implements WebSearchProvider {
  readonly name = "duckduckgo_html";

  constructor(private readonly fetchImpl: typeof fetch) {}

  async search(
    query: string,
    maxResults: number,
    signal?: AbortSignal,
  ): Promise<readonly WebSearchResult[]> {
    const url = `https://duckduckgo.com/html/?q=${encodeURIComponent(query)}`;
    const response = await this.fetchImpl(url, {
      headers: { "user-agent": userAgent },
      signal,
    });
    if (!response.ok) {
      throw new Error(`DuckDuckGo search failed with HTTP ${response.status}`);
    }
    return parseDuckDuckGoResults(await response.text()).slice(0, maxResults);
  }
}

interface ExtractUrlOptions {
  policy: WebNetworkPolicy;
  maxExtractChars: number;
  maxExtractBytes: number;
  maxRedirects: number;
  signal?: AbortSignal;
}

interface FetchPublicUrlResult {
  response: Response;
  finalUrl: string;
  redirectCount: number;
}

async function extractUrl(
  fetchImpl: typeof fetch,
  rawUrl: string,
  options: ExtractUrlOptions,
): Promise<WebExtractResult> {
  try {
    const { response, finalUrl, redirectCount } = await fetchPublicUrl(
      fetchImpl,
      rawUrl,
      options,
    );
    if (!response.ok) {
      throw new WebToolDeniedError("http_error", `HTTP ${response.status}`);
    }
    const contentType = response.headers.get("content-type") ?? undefined;
    if (!isSafeTextContentType(contentType)) {
      throw new WebToolDeniedError(
        "unsupported_content_type",
        `unsupported content type ${contentType ?? "unknown"}`,
      );
    }
    const { text, truncatedBytes } = await readResponseText(
      response,
      options.maxExtractBytes,
    );
    const title = htmlTitle(text) ?? finalUrl;
    const content = htmlToText(text).slice(0, options.maxExtractChars);
    return {
      url: rawUrl,
      finalUrl,
      title,
      content,
      status: response.status,
      contentType,
      redirectCount,
      truncated:
        truncatedBytes || htmlToText(text).length > options.maxExtractChars,
    };
  } catch (error) {
    return {
      url: rawUrl,
      title: rawUrl,
      content: "",
      redirectCount: 0,
      truncated: false,
      error: errorMessage(error),
      reasonCode:
        error instanceof WebToolDeniedError ? error.reasonCode : "fetch_failed",
    };
  }
}

async function fetchPublicUrl(
  fetchImpl: typeof fetch,
  rawUrl: string,
  options: ExtractUrlOptions,
): Promise<FetchPublicUrlResult> {
  let current = rawUrl;
  for (
    let redirectCount = 0;
    redirectCount <= options.maxRedirects;
    redirectCount += 1
  ) {
    await assertSafePublicUrl(current, options.policy);
    const response = await fetchImpl(current, {
      headers: { "user-agent": userAgent },
      redirect: "manual",
      signal: options.signal,
    });
    if (!isRedirect(response.status)) {
      return { response, finalUrl: current, redirectCount };
    }
    const location = response.headers.get("location");
    if (!location) {
      return { response, finalUrl: current, redirectCount };
    }
    current = new URL(location, current).toString();
  }
  throw new WebToolDeniedError("too_many_redirects", "too many redirects");
}

export async function assertSafePublicUrl(
  rawUrl: string,
  policyInput: Partial<WebNetworkPolicy> = {},
): Promise<void> {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    throw new WebToolDeniedError("invalid_url", "invalid URL");
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new WebToolDeniedError(
      "unsupported_scheme",
      "only http/https URLs are allowed",
    );
  }
  if (url.username || url.password) {
    throw new WebToolDeniedError(
      "credentialed_url",
      "credentialed URLs are blocked",
    );
  }

  const policy: WebNetworkPolicy = {
    allowPrivateNet: policyInput.allowPrivateNet ?? false,
    allowedNonstandardPorts:
      policyInput.allowedNonstandardPorts ?? new Set<number>(),
    resolveHostAddresses:
      policyInput.resolveHostAddresses ?? defaultResolveHostAddresses,
  };
  const port = url.port ? Number.parseInt(url.port, 10) : undefined;
  if (port !== undefined && !isAllowedPort(url, port, policy)) {
    throw new WebToolDeniedError(
      "nonstandard_port",
      "non-standard URL ports are blocked",
    );
  }

  const host = normalizedHost(url);
  if (host === "localhost" || host.endsWith(".localhost")) {
    if (!policy.allowPrivateNet) {
      throw new WebToolDeniedError(
        "private_network",
        "localhost URLs are blocked",
      );
    }
    return;
  }

  const ipKind = isIP(host);
  const addresses =
    ipKind === 0
      ? await policy.resolveHostAddresses(host)
      : [{ address: host, family: ipKind as 4 | 6 }];
  if (addresses.length === 0) {
    throw new WebToolDeniedError(
      "dns_resolution_failed",
      "hostname did not resolve",
    );
  }
  if (!policy.allowPrivateNet) {
    for (const address of addresses) {
      if (isPrivateAddress(address)) {
        throw new WebToolDeniedError(
          "private_network",
          "private-network URLs are blocked",
        );
      }
    }
  }
}

function webNetworkPolicy(context: WebToolContext): WebNetworkPolicy {
  const env = context.env ?? process.env;
  const allowPrivateNet =
    context.allowPrivateNet ??
    (env.RUSTY_CREW_ALLOW_PRIVATE_NET === "1" ||
      env.PI_CREW_ALLOW_PRIVATE_NET === "1");
  return {
    allowPrivateNet,
    allowedNonstandardPorts: new Set(context.allowedNonstandardPorts ?? []),
    resolveHostAddresses:
      context.resolveHostAddresses ?? defaultResolveHostAddresses,
  };
}

async function defaultResolveHostAddresses(
  hostname: string,
): Promise<readonly ResolvedAddress[]> {
  return lookup(hostname, { all: true, verbatim: true }) as Promise<
    ResolvedAddress[]
  >;
}

function normalizedHost(url: URL): string {
  return url.hostname.toLowerCase().replace(/^\[/, "").replace(/\]$/, "");
}

function isAllowedPort(
  url: URL,
  port: number,
  policy: WebNetworkPolicy,
): boolean {
  if (policy.allowedNonstandardPorts.has(port)) {
    return true;
  }
  return (
    (url.protocol === "http:" && port === 80) ||
    (url.protocol === "https:" && port === 443)
  );
}

function isPrivateAddress(address: ResolvedAddress): boolean {
  if (address.family === 6) {
    return isPrivateIpv6(address.address);
  }
  return isPrivateIpv4(address.address);
}

function isPrivateIpv4(address: string): boolean {
  const parts = address.split(".").map((part) => Number.parseInt(part, 10));
  if (
    parts.length !== 4 ||
    parts.some((part) => !Number.isInteger(part) || part < 0 || part > 255)
  ) {
    return true;
  }
  const [a = 0, b = 0] = parts;
  if (a === 0 || a === 10 || a === 127 || a >= 224) {
    return true;
  }
  if (a === 100 && b >= 64 && b <= 127) {
    return true;
  }
  if (a === 169 && b === 254) {
    return true;
  }
  if (a === 172 && b >= 16 && b <= 31) {
    return true;
  }
  if (a === 192 && b === 168) {
    return true;
  }
  if (a === 198 && (b === 18 || b === 19)) {
    return true;
  }
  return a >= 240;
}

function isPrivateIpv6(address: string): boolean {
  const normalized = address.toLowerCase();
  if (
    normalized === "::" ||
    normalized === "::1" ||
    normalized.startsWith("fc") ||
    normalized.startsWith("fd") ||
    normalized.startsWith("fe80:")
  ) {
    return true;
  }
  return mappedIpv4IsPrivate(normalized);
}

function mappedIpv4IsPrivate(address: string): boolean {
  if (!address.startsWith("::ffff:")) {
    return false;
  }
  const suffix = address.slice("::ffff:".length);
  if (suffix.includes(".")) {
    return isPrivateIpv4(suffix);
  }
  const parts = suffix.split(":");
  const high = Number.parseInt(parts.at(-2) ?? "", 16);
  const low = Number.parseInt(parts.at(-1) ?? "", 16);
  if (!Number.isInteger(high) || !Number.isInteger(low)) {
    return true;
  }
  const ipv4 = [high >> 8, high & 0xff, low >> 8, low & 0xff].join(".");
  return isPrivateIpv4(ipv4);
}

function isRedirect(status: number): boolean {
  return status >= 300 && status < 400;
}

function isSafeTextContentType(contentType: string | undefined): boolean {
  if (!contentType) {
    return true;
  }
  const normalized = contentType.toLowerCase().split(";")[0]?.trim() ?? "";
  return (
    normalized.startsWith("text/") ||
    normalized === "application/json" ||
    normalized.endsWith("+json") ||
    normalized === "application/xhtml+xml" ||
    normalized === "application/xml" ||
    normalized.endsWith("+xml")
  );
}

async function readResponseText(
  response: Response,
  maxBytes: number,
): Promise<{ text: string; truncatedBytes: boolean }> {
  if (!response.body) {
    const text = await response.text();
    return {
      text: text.slice(0, maxBytes),
      truncatedBytes: Buffer.byteLength(text, "utf8") > maxBytes,
    };
  }

  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  let truncatedBytes = false;
  while (total < maxBytes) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    const remaining = maxBytes - total;
    if (value.byteLength > remaining) {
      chunks.push(value.slice(0, remaining));
      total += remaining;
      truncatedBytes = true;
      await reader.cancel();
      break;
    }
    chunks.push(value);
    total += value.byteLength;
  }
  if (total >= maxBytes) {
    truncatedBytes = true;
  }
  return {
    text: new TextDecoder().decode(Buffer.concat(chunks)),
    truncatedBytes,
  };
}

function htmlTitle(html: string): string | null {
  const match = /<title[^>]*>(.*?)<\/title>/is.exec(html);
  return match === null ? null : decodeHtml(stripTags(match[1] ?? "")).trim();
}

function htmlToText(html: string): string {
  return decodeHtml(
    stripTags(
      html
        .replace(/<script[\s\S]*?<\/script>/gi, "")
        .replace(/<style[\s\S]*?<\/style>/gi, ""),
    ),
  )
    .replace(/[ \t]+/g, " ")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function configuredSearxngUrl(
  context: WebSearchToolContext,
): string | undefined {
  const env = context.env ?? process.env;
  const explicit = context.searxngUrl?.trim();
  if (explicit) {
    return explicit;
  }
  const rustyCrew = env.RUSTY_CREW_SEARXNG_URL?.trim();
  if (rustyCrew) {
    return rustyCrew;
  }
  const legacy = env.PI_CREW_SEARXNG_URL?.trim();
  return legacy && legacy.length > 0 ? legacy : undefined;
}

function parseDuckDuckGoResults(html: string): readonly WebSearchResult[] {
  const results: WebSearchResult[] = [];
  const blocks = html.split(/<a[^>]+class="result__a"/g).slice(1);
  for (const block of blocks) {
    const href = /href="([^"]+)"/.exec(block)?.[1];
    const titleHtml = />(.*?)<\/a>/s.exec(block)?.[1] ?? "";
    const snippetHtml =
      /class="result__snippet"[^>]*>(.*?)<\/a>/s.exec(block)?.[1] ??
      /class="result__snippet"[^>]*>(.*?)<\/div>/s.exec(block)?.[1] ??
      "";
    if (!href) {
      continue;
    }
    results.push({
      title: decodeHtml(stripTags(titleHtml)),
      url: normalizeDuckDuckGoUrl(decodeHtml(href)),
      snippet: decodeHtml(stripTags(snippetHtml)),
    });
  }
  return normalizeResults(results);
}

function normalizeDuckDuckGoUrl(value: string): string {
  try {
    const url = new URL(value, "https://duckduckgo.com");
    return url.searchParams.get("uddg") ?? url.toString();
  } catch {
    return value;
  }
}

function normalizeResults(
  results: readonly WebSearchResult[],
): readonly WebSearchResult[] {
  return results.flatMap((result) => {
    const url = result.url.trim();
    if (!url) {
      return [];
    }
    return [
      {
        title: singleLine(result.title.trim() || url),
        url,
        snippet: singleLine(result.snippet.trim()),
      },
    ];
  });
}

function singleLine(value: string): string {
  return value.replace(/\s+/g, " ");
}

function stripTags(html: string): string {
  return html
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/p>/gi, "\n")
    .replace(/<[^>]+>/g, "");
}

function decodeHtml(text: string): string {
  return text
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function clampInt(value: number, min: number, max: number): number {
  if (!Number.isFinite(value)) {
    return min;
  }
  return Math.max(min, Math.min(max, Math.trunc(value)));
}

function resultDetails(
  details: WebSearchToolDetails,
): BrainToolResult<WebSearchToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function extractResultDetails(
  details: WebExtractToolDetails,
): BrainToolResult<WebExtractToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

class WebToolDeniedError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
