import assert from "node:assert/strict";
import { createHash } from "node:crypto";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const certificateDialect =
  process.env.RUSTY_CREW_REASONING_CERT_DIALECT ?? "kimi";
const taskNumber = certificateDialect === "deepseek" ? "6020" : "6003";
const providerAlias =
  process.env.RUSTY_CREW_REASONING_CERT_PROVIDER_ALIAS ??
  (certificateDialect === "deepseek" ? "deepseek-flash" : "kimi-k2.7");
const suffix = Date.now().toString(36);
const profileId = `${certificateDialect}-reasoning-cert-${suffix}`;
let sessionId: string | undefined;
let originalProvider: Record<string, unknown> | undefined;
let providerUpdated = false;

if (new URL(baseUrl).port !== "9348") {
  throw new Error("reasoning live smoke requires debug port 9348");
}
if (certificateDialect !== "kimi" && certificateDialect !== "deepseek") {
  throw new Error("reasoning live smoke supports kimi or deepseek dialects");
}
if (certificateDialect === "kimi" && providerAlias !== "kimi-k2.7") {
  throw new Error("Kimi reasoning live smoke requires provider kimi-k2.7");
}

try {
  originalProvider = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
  );
  if (certificateDialect === "deepseek") {
    providerUpdated = true;
    const updated = await apiData<Record<string, unknown>>(
      "PATCH",
      `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}?refresh=apply`,
      providerWriteBody(originalProvider, {
        chatCompletionsDialect: "deepseek",
        thinkingMode: "enabled",
        reasoningHistory: "tool_calls_only",
      }),
    );
    assert.equal(
      nested(updated, "provider", "chatCompletionsDialect"),
      "deepseek",
    );
    assert.equal(
      nested(updated, "provider", "reasoningHistory"),
      "tool_calls_only",
    );
  } else {
    assert.equal(nested(originalProvider, "chatCompletionsDialect"), "kimi");
    assert.equal(nested(originalProvider, "reasoningHistory"), "preserve_all");
  }

  const created = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/profiles",
    {
      profileId,
      displayName: `${certificateDialect} reasoning certification ${suffix}`,
      providerAlias,
      kind: "full",
      localToolProfileId: "code_read",
      reason: `task-${taskNumber} ${certificateDialect} live reasoning certification`,
    },
  );
  sessionId = nestedString(created, "outcome", "result", "sessionId");
  assert.ok(sessionId, "profile creation must return a session id");

  const first = await sendAndWait(
    sessionId,
    [
      "Perform this verification using tools, not prior knowledge.",
      "First call git_status for /home/dev/rusty-crew.",
      "Only after that tool result returns, call read_file for /home/dev/rusty-crew/README.md.",
      "Do not issue those tool calls in parallel.",
      `Then answer with the exact marker ${certificateDialect.toUpperCase()}_REASONING_${suffix} and one short sentence.`,
    ].join("\n"),
  );
  const firstTools = successfulTools(first.events);
  assert.deepEqual(
    firstTools.slice(0, 2).map((tool) => tool.name),
    ["git_status", "read_file"],
    `expected sequential git_status/read_file calls: ${JSON.stringify(firstTools)}`,
  );
  assert.notEqual(firstTools[0]?.callId, firstTools[1]?.callId);

  const firstDebug = await exactProviderDebug(sessionId, first.events);
  const firstRequests = requestSamples(firstDebug);
  assert.ok(
    firstRequests.length >= 3,
    `two sequential tool rounds require at least three provider requests, got ${firstRequests.length}`,
  );
  const firstReplayReasoning = assistantToolReasoning(firstRequests[1]);
  assert.ok(
    firstReplayReasoning.length > 0,
    "second provider request must replay first tool-call reasoning_content",
  );
  const firstReasoningHash = sha256(firstReplayReasoning);
  assert.equal(
    assistantToolReasoning(firstRequests[2]),
    firstReplayReasoning,
    "later provider requests must retain exact first-round reasoning_content",
  );
  const firstToolReasoning = assistantToolReasonings(
    firstRequests[firstRequests.length - 1],
  );
  assert.ok(
    firstToolReasoning.length >= 2,
    "both sequential assistant tool calls must retain reasoning_content",
  );

  const second = await sendAndWait(
    sessionId,
    `Without calling tools, confirm the earlier README verification with marker ${certificateDialect.toUpperCase()}_HISTORY_${suffix}.`,
    first.cursor,
  );
  assert.equal(successfulTools(second.events).length, 0);
  const secondDebug = await exactProviderDebug(sessionId, second.events);
  const secondRequests = requestSamples(secondDebug);
  assert.ok(secondRequests.length >= 1);
  const restoredReasoning = allReasoning(secondRequests[0]);
  if (certificateDialect === "deepseek") {
    assert.deepEqual(
      assistantToolReasonings(secondRequests[0]),
      firstToolReasoning,
      "DeepSeek second wake must restore exact reasoning only on tool-call assistant messages",
    );
    assert.deepEqual(
      assistantNonToolReasonings(secondRequests[0]),
      [],
      "DeepSeek tool_calls_only history must omit non-tool assistant reasoning",
    );
  } else {
    assert.ok(
      restoredReasoning.includes(firstReplayReasoning),
      "second wake must restore the exact prior reasoning_content under preserve_all",
    );
  }

  console.log(
    JSON.stringify(
      {
        baseUrl,
        certificateDialect,
        providerAlias,
        profileId,
        sessionId,
        firstWakeProviderRequests: firstRequests.length,
        firstWakeTools: firstTools.slice(0, 2).map((tool) => tool.name),
        firstReasoningSha256: firstReasoningHash,
        secondWakeProviderRequests: secondRequests.length,
        secondWakeRestoredReasoningSha256: sha256(firstReplayReasoning),
        rawReasoningPersistedInEvidence: false,
      },
      null,
      2,
    ),
  );
} finally {
  if (sessionId !== undefined) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: `task-${taskNumber} reasoning certification cleanup`,
      },
    ).catch(() => undefined);
  }
  if (providerUpdated && originalProvider !== undefined) {
    const current = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
    );
    await apiData<Record<string, unknown>>(
      "PATCH",
      `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}?refresh=apply`,
      providerWriteBody(originalProvider, {
        expectedRevision: nested(current, "revision"),
      }),
    );
  }
}

interface ChatEvent {
  event_id?: string;
  kind?: string;
  payload?: Record<string, unknown>;
}

interface ToolResult {
  callId: string;
  name: string;
}

interface SendResult {
  cursor: string;
  events: ChatEvent[];
}

async function sendAndWait(
  currentSessionId: string,
  body: string,
  initialCursor?: string,
): Promise<SendResult> {
  let cursor = initialCursor ?? `${currentSessionId}:0`;
  const key = `task-${taskNumber}:${currentSessionId}:${Date.now()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/messages`,
    {
      actor: { id: `task-${taskNumber}-operator`, kind: "human" },
      body,
      client_message_id: key,
      reason: `task-${taskNumber} ${certificateDialect} live reasoning certification`,
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);

  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    const items = Array.isArray(page.items) ? (page.items as ChatEvent[]) : [];
    events.push(...items);
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    const finished = events.find(
      (event) => event.kind === "assistant_turn_finished",
    );
    if (finished) {
      assert.notEqual(
        finished.payload?.status,
        "failed",
        JSON.stringify(events, null, 2),
      );
      await waitForExactDebug(currentSessionId, events, cursor);
      return { cursor, events };
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for ${currentSessionId}`);
}

async function waitForExactDebug(
  currentSessionId: string,
  events: ChatEvent[],
  initialCursor: string,
): Promise<void> {
  let cursor = initialCursor;
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    if (await hasExactDebug(currentSessionId, events)) return;
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=100`,
    );
    events.push(
      ...(Array.isArray(page.items) ? (page.items as ChatEvent[]) : []),
    );
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    await delay(100);
  }
  throw new Error("timed out waiting for exact provider request debug detail");
}

async function hasExactDebug(
  currentSessionId: string,
  events: ChatEvent[],
): Promise<boolean> {
  for (const detailId of providerDebugIds(events)) {
    const response = await api(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/provider-requests/${encodeURIComponent(detailId)}`,
    );
    if (response.status >= 400) continue;
    if (
      nested(response.json, "data", "request", "value", "boundary") ===
      "rust_chat_completions_request"
    ) {
      return true;
    }
  }
  return false;
}

async function exactProviderDebug(
  currentSessionId: string,
  events: ChatEvent[],
): Promise<Record<string, unknown>> {
  for (const detailId of providerDebugIds(events)) {
    const detail = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/provider-requests/${encodeURIComponent(detailId)}`,
    );
    if (
      nested(detail, "request", "value", "boundary") ===
      "rust_chat_completions_request"
    ) {
      return detail;
    }
  }
  throw new Error("exact Rust Chat Completions request debug detail missing");
}

function providerDebugIds(events: ChatEvent[]): string[] {
  const ids = new Set<string>();
  for (const event of events) {
    const metadataCandidates = [
      event.payload?.metadata,
      event.payload?.metadataJson,
      event.payload?.metadata_json,
    ];
    for (const candidate of metadataCandidates) {
      const metadata = parseObject(candidate);
      const id = metadata?.provider_request_debug_detail_id;
      if (typeof id === "string") ids.add(id);
    }
  }
  return [...ids];
}

function successfulTools(events: ChatEvent[]): ToolResult[] {
  return events
    .filter(
      (event) =>
        event.kind === "tool_call_completed" &&
        event.payload?.is_error !== true,
    )
    .map((event) => ({
      callId: String(
        event.payload?.tool_call_id ?? event.payload?.call_id ?? "",
      ),
      name: String(event.payload?.tool_name ?? ""),
    }));
}

function requestSamples(detail: Record<string, unknown>): unknown[] {
  const requests = nested(detail, "request", "value", "requests");
  assert.ok(
    Array.isArray(requests),
    "provider debug detail must have requests",
  );
  return requests;
}

function assistantToolReasoning(request: unknown): string {
  const messages = nested(request, "messages");
  if (!Array.isArray(messages)) return "";
  const message = messages.find(
    (candidate) =>
      nested(candidate, "role") === "assistant" &&
      Array.isArray(nested(candidate, "tool_calls")) &&
      (nested(candidate, "tool_calls") as unknown[]).length > 0,
  );
  const reasoning = nested(message, "reasoning_content");
  return typeof reasoning === "string" ? reasoning : "";
}

function assistantToolReasonings(request: unknown): string[] {
  const messages = nested(request, "messages");
  if (!Array.isArray(messages)) return [];
  return messages
    .filter(
      (message) =>
        nested(message, "role") === "assistant" &&
        Array.isArray(nested(message, "tool_calls")) &&
        (nested(message, "tool_calls") as unknown[]).length > 0,
    )
    .map((message) => nested(message, "reasoning_content"))
    .filter((value): value is string => typeof value === "string");
}

function assistantNonToolReasonings(request: unknown): string[] {
  const messages = nested(request, "messages");
  if (!Array.isArray(messages)) return [];
  return messages
    .filter(
      (message) =>
        nested(message, "role") === "assistant" &&
        (!Array.isArray(nested(message, "tool_calls")) ||
          (nested(message, "tool_calls") as unknown[]).length === 0),
    )
    .map((message) => nested(message, "reasoning_content"))
    .filter((value): value is string => typeof value === "string");
}

function providerWriteBody(
  provider: Record<string, unknown>,
  overrides: Record<string, unknown>,
): Record<string, unknown> {
  return {
    status: provider.status,
    protocol: provider.protocol,
    providerKind: provider.providerKind,
    displayName: provider.displayName,
    description: provider.description,
    baseUrl: provider.baseUrl,
    modelId: provider.modelId,
    contextWindowTokens: provider.contextWindowTokens,
    maxOutputTokens: provider.maxOutputTokens,
    temperatureMilli: provider.temperatureMilli,
    reasoningEffort: provider.reasoningEffort,
    reasoningFormat: provider.reasoningFormat,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    metadataJson: provider.metadataJson,
    expectedRevision: provider.revision,
    ...overrides,
  };
}

function allReasoning(request: unknown): string {
  const messages = nested(request, "messages");
  if (!Array.isArray(messages)) return "";
  return messages
    .map((message) => nested(message, "reasoning_content"))
    .filter((value): value is string => typeof value === "string")
    .join("\n");
}

function parseObject(value: unknown): Record<string, unknown> | undefined {
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  if (typeof value !== "string") return undefined;
  try {
    const parsed = JSON.parse(value) as unknown;
    return typeof parsed === "object" &&
      parsed !== null &&
      !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : undefined;
  } catch {
    return undefined;
  }
}

function nested(value: unknown, ...path: string[]): unknown {
  let current = value;
  for (const key of path) {
    if (
      typeof current !== "object" ||
      current === null ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function nestedString(value: unknown, ...path: string[]): string | undefined {
  const result = nested(value, ...path);
  return typeof result === "string" ? result : undefined;
}

function sha256(value: string): string {
  return createHash("sha256").update(value).digest("hex");
}

async function apiData<T>(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  assert.equal(response.json.ok, true, response.text);
  return response.json.data as T;
}

async function api(
  method: "GET" | "POST" | "PATCH",
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(300_000),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  try {
    json = JSON.parse(text) as Record<string, unknown>;
  } catch {
    // Assertions retain the raw response.
  }
  return { status: response.status, text, json };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
