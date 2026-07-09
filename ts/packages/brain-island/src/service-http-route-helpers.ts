import type { IncomingMessage, ServerResponse } from "node:http";
import type { RustyCrewServiceConfig } from "./service-config.js";
import {
  isRawServiceRouteResult,
  type ServiceRouteResult,
} from "./service-route-results.js";

const DEV_NO_AUTH_CONTROL_TOKEN = "__rusty_crew_dev_no_auth__";

export function writeJsonResponse(
  response: ServerResponse,
  result: ServiceRouteResult,
): void {
  if (isRawServiceRouteResult(result)) {
    result.write(response);
    return;
  }
  for (const [name, value] of Object.entries(result.headers)) {
    response.setHeader(name, value);
  }
  response.statusCode = result.status;
  response.end(
    typeof result.body === "string" ? result.body : JSON.stringify(result.body),
  );
}

export function chatCorsPreflightResponse(
  request: IncomingMessage,
): ServiceRouteResult {
  return {
    status: 204,
    headers: chatCorsHeaders(request),
    body: "",
  };
}

export function withChatCors<T extends ServiceRouteResult>(
  result: T,
  request: IncomingMessage,
): T {
  if (isRawServiceRouteResult(result)) return result;
  return {
    ...result,
    headers: {
      ...result.headers,
      ...chatCorsHeaders(request),
    },
  };
}

export function chatCorsHeaders(
  request: IncomingMessage,
): Record<string, string> {
  const origin = stringHeader(request, "origin") ?? "*";
  return {
    "access-control-allow-origin": origin,
    "access-control-allow-methods": "GET,POST,PATCH,DELETE,OPTIONS",
    "access-control-allow-headers":
      "authorization,content-type,idempotency-key,last-event-id,x-request-id",
    "access-control-expose-headers": "content-type",
    "access-control-max-age": "600",
    vary: origin === "*" ? "Origin" : "Origin",
  };
}

export function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

export function numberParam(url: URL, key: string): number | undefined {
  const value = stringParam(url, key);
  if (value === undefined) return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

export function pageParams(url: URL): { limit?: number; offset?: number } {
  const limit = optionalInteger(url.searchParams.get("limit"));
  const offset = optionalInteger(url.searchParams.get("offset"));
  return {
    ...(limit === undefined ? {} : { limit }),
    ...(offset === undefined ? {} : { offset }),
  };
}

export function optionalInteger(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}

export function requestId(request: IncomingMessage): string {
  const value = request.headers["x-request-id"];
  return typeof value === "string" && value.trim()
    ? value.trim()
    : `req_${Date.now()}`;
}

export function stringHeader(
  request: IncomingMessage,
  name: string,
): string | undefined {
  const value = request.headers[name.toLowerCase()];
  if (Array.isArray(value)) return value.find((candidate) => candidate.trim());
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

export function headers(
  request: IncomingMessage,
): Record<string, string | undefined> {
  const result: Record<string, string | undefined> = {};
  for (const [name, value] of Object.entries(request.headers)) {
    result[name] = Array.isArray(value) ? value.join(", ") : value;
  }
  return result;
}

export function controlHeaders(
  request: IncomingMessage,
  config: RustyCrewServiceConfig,
): Record<string, string | undefined> {
  const result = headers(request);
  if (!configRequiresAuth(config)) {
    result.authorization = `Bearer ${DEV_NO_AUTH_CONTROL_TOKEN}`;
  }
  return result;
}

export function controlBearerToken(config: RustyCrewServiceConfig): string {
  return configRequiresAuth(config)
    ? (config.admin.token ?? "")
    : DEV_NO_AUTH_CONTROL_TOKEN;
}

export function configRequiresAuth(config: RustyCrewServiceConfig): boolean {
  return config.admin.authMode !== "none";
}

export function isAuthorized(
  request: IncomingMessage,
  token: string | undefined,
  config?: RustyCrewServiceConfig,
): boolean {
  if (config && !configRequiresAuth(config)) return true;
  return Boolean(token) && request.headers.authorization === `Bearer ${token}`;
}

export async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
    if (Buffer.concat(chunks).byteLength > 1024 * 1024) {
      throw new Error("admin request body exceeds 1 MiB");
    }
  }
  const raw = Buffer.concat(chunks).toString("utf8").trim();
  if (!raw) return {};
  return JSON.parse(raw) as unknown;
}
