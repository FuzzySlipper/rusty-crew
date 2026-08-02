import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const providerAlias =
  process.env.RUSTY_CREW_DEEPSEEK_RESPONSES_PROVIDER_ALIAS ??
  "deepseek-flash-responses";
const serviceUnit =
  process.env.RUSTY_CREW_DEBUG_SERVICE_UNIT ?? "rusty-crew-debug.service";
const evidenceRoot =
  process.env.RUSTY_CREW_DEEPSEEK_RESPONSES_EVIDENCE_ROOT ??
  "/home/system/rusty-crew-debug/evidence/task-6530";
const restartProof =
  process.env.RUSTY_CREW_DEEPSEEK_RESPONSES_RESTART_PROOF === "1";
const keepProfile =
  process.env.RUSTY_CREW_DEEPSEEK_RESPONSES_KEEP_PROFILE === "1";
const suffix = Date.now().toString(36);
const profileId = `task-6530-deepseek-responses-${suffix}`;
let sessionId: string | undefined;

assert.equal(new URL(baseUrl).port, "9348", "live certification is debug-only");
assert.equal(serviceUnit, "rusty-crew-debug.service");

try {
  const provider = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/model-providers/${encodeURIComponent(providerAlias)}`,
  );
  assert.equal(provider.status, "active");
  assert.equal(provider.protocol, "responses");
  assert.equal(provider.responsesDialect, "deepseek");
  assert.equal(provider.modelId, "deepseek-v4-flash");
  assert.equal(nested(provider, "credential", "hasSecret"), true);
  assert.equal(nested(provider, "credential", "kind"), "api_key");
  assert.equal(JSON.stringify(provider).includes('api_key":'), false);

  const created = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/profiles",
    {
      profileId,
      displayName: `Task 6530 DeepSeek Responses ${suffix}`,
      providerAlias,
      kind: "full",
      localToolProfileId: "full_coding_agent",
      reason: "task 6530 DeepSeek Responses live certification",
    },
  );
  sessionId = nestedString(created, "outcome", "result", "sessionId");
  assert.ok(sessionId, "profile creation must return a session id");

  const simple = await sendAndWait(
    sessionId,
    `Reply with the exact marker DEEPSEEK_SIMPLE_${suffix} and one short sentence.`,
  );
  assert.match(
    assistantText(simple.events),
    new RegExp(`DEEPSEEK_SIMPLE_${suffix}`),
  );
  assert.ok(
    simple.events.some((event) => event.kind === "assistant_reasoning_delta"),
    "DeepSeek reasoning text must project as reasoning events",
  );

  const continuity = await sendAndWait(
    sessionId,
    `Without tools, recall the exact marker from your immediately previous answer and then print CONTINUITY_${suffix}.`,
    simple.cursor,
  );
  const continuityText = assistantText(continuity.events);
  assert.match(continuityText, new RegExp(`DEEPSEEK_SIMPLE_${suffix}`));
  assert.match(continuityText, new RegExp(`CONTINUITY_${suffix}`));

  const sequential = await sendAndWait(
    sessionId,
    [
      "Use tools and wait for each result before issuing the next call.",
      "First call git_status with /home/dev/rusty-crew.",
      "Then call read_file with /home/dev/rusty-crew/package.json.",
      "Then call search_files for the literal `Rusty Crew` under /home/dev/rusty-crew/docs with max_results 2.",
      `Finally print SEQUENTIAL_${suffix}.`,
    ].join("\n"),
    continuity.cursor,
  );
  const sequentialTools = successfulTools(sequential.events);
  assert.ok(
    sequentialTools.length >= 3,
    "sequential scenario must complete at least three ordered tool calls",
  );
  assert.equal(sequentialTools[0]?.name, "git_status");
  assert.ok(sequentialTools.some((tool) => tool.name === "read_file"));
  assert.ok(
    sequentialTools.some((tool) =>
      ["search_files", "terminal"].includes(tool.name),
    ),
    "repository search may use either the dedicated search tool or terminal",
  );
  assert.match(
    assistantText(sequential.events),
    new RegExp(`SEQUENTIAL_${suffix}`),
  );

  const parallel = await sendAndWait(
    sessionId,
    [
      "In one parallel function-call batch, call read_file for /home/dev/rusty-crew/README.md and read_file for /home/dev/rusty-crew/package.json.",
      "Do not wait for one result before issuing the other call.",
      `After both results return, print PARALLEL_${suffix}.`,
    ].join("\n"),
    sequential.cursor,
  );
  const parallelTools = successfulTools(parallel.events).filter(
    (tool) => tool.name === "read_file",
  );
  assert.equal(
    parallelTools.length,
    2,
    "parallel batch must complete two reads",
  );
  assert.equal(new Set(parallelTools.map((tool) => tool.callId)).size, 2);
  assert.match(
    assistantText(parallel.events),
    new RegExp(`PARALLEL_${suffix}`),
  );

  const recovery = await sendAndWait(
    sessionId,
    [
      "Call read_file with the directory path /home/dev/rusty-crew (not a file) so the tool returns an expected EISDIR error.",
      "After that expected tool error, recover by calling read_file for /home/dev/rusty-crew/README.md.",
      `Then print RECOVERED_${suffix} and briefly report that the first tool failed.`,
    ].join("\n"),
    parallel.cursor,
  );
  assert.ok(
    recovery.events.some(
      (event) =>
        event.kind === "tool_call_failed" ||
        (event.kind === "tool_call_completed" &&
          event.payload.is_error === true),
    ),
    "expected read-directory tool failure",
  );
  assert.ok(
    successfulTools(recovery.events).some((tool) => tool.name === "read_file"),
    "turn must recover to a successful tool call",
  );
  assert.match(
    assistantText(recovery.events),
    new RegExp(`RECOVERED_${suffix}`),
  );

  let restart: ScenarioResult | undefined;
  if (restartProof) {
    const diagnostic = await profileDiagnostic(profileId);
    assert.equal(
      nested(diagnostic, "modelProvider", "workQuantumContinuationRounds"),
      1,
      "restart proof requires RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS=1",
    );
    restart = await sendAndRestart(
      sessionId,
      recovery.cursor,
      [
        "Complete these calls sequentially, waiting after every result.",
        "Call git_status for /home/dev/rusty-crew.",
        "Then read_file /home/dev/rusty-crew/package.json.",
        "Then read_file /home/dev/rusty-crew/README.md.",
        "Then search_files for `responsesDialect` under /home/dev/rusty-crew with max_results 3.",
        `Finally print RESTART_CONTINUED_${suffix}.`,
      ].join("\n"),
    );
    assert.ok(
      restart.events.some(
        (event) => event.kind === "logical_turn_queued_to_continue",
      ),
      "turn must yield before the planned restart",
    );
    assert.ok(successfulTools(restart.events).length >= 4);
    assert.match(
      assistantText(restart.events),
      new RegExp(`RESTART_CONTINUED_${suffix}`),
    );
  }

  const long = await sendAndWait(
    sessionId,
    [
      "Perform a bounded architecture inventory using real tools.",
      "Read Cargo.toml, package.json, README.md, docs/model-providers.md, and crates/brains/openai-responses/Cargo.toml.",
      "Search the repository for ResponsesProviderDialect and response.reasoning_text.delta.",
      "Inspect git status last.",
      `Then print LONG_AGENTIC_${suffix} and summarize the Rust/TypeScript ownership in no more than six bullets.`,
    ].join("\n"),
    restart?.cursor ?? recovery.cursor,
  );
  assert.ok(
    successfulTools(long.events).length >= 7,
    "long scenario needs substantial tool traffic",
  );
  assert.match(
    assistantText(long.events),
    new RegExp(`LONG_AGENTIC_${suffix}`),
  );

  const allEvents = [
    ...simple.events,
    ...continuity.events,
    ...sequential.events,
    ...parallel.events,
    ...recovery.events,
    ...(restart?.events ?? []),
    ...long.events,
  ];
  const debugDetails = await exactProviderDebugDetails(sessionId, allEvents);
  const requests = debugDetails.flatMap(providerRequests);
  if (!restartProof) {
    assert.ok(
      debugDetails.length >= 2,
      "early requests must remain exactly inspectable before later tool-heavy prompts reach the bounded debug-cache preview limit",
    );
    assert.ok(requests.length >= 2);
  }
  for (const request of requests) assertDeepseekRequest(request);
  if (!restartProof) {
    assert.ok(
      requests.some((request) => inputItems(request).length > 4),
      "later stateless wakes must replay prior messages and tool history",
    );
  }

  const diagnostic = await profileDiagnostic(profileId);
  assert.equal(
    nested(diagnostic, "modelProvider", "responsesDialect"),
    "deepseek",
  );
  const metrics = nestedArray(diagnostic, "responsesWakeMetrics").filter(
    (item) => item.sessionId === sessionId,
  );
  assert.ok(metrics.length >= 6);
  assert.ok(metrics.every((metric) => metric.providerDialect === "deepseek"));
  assert.ok(metrics.every((metric) => metric.effectiveStrategyId === "replay"));
  if (!restartProof) {
    const parallelWakeIds = wakeIds(parallel.events);
    const parallelMetrics = metrics.find((metric) =>
      parallelWakeIds.includes(String(metric.wakeId)),
    );
    assert.equal(
      parallelMetrics?.providerRequestCount,
      2,
      "two parallel calls must be emitted by one provider response and followed by one continuation request",
    );
  }
  assert.ok(
    metrics.some(
      (metric) =>
        Number(
          nested(
            metric,
            "providerEventCounts",
            "response.reasoning_text.delta",
          ),
        ) > 0,
    ),
    "diagnostics must retain the DeepSeek reasoning event name",
  );
  assert.ok(metrics.every((metric) => Number(metric.inputTokens) > 0));
  assert.ok(metrics.some((metric) => Number(metric.reasoningOutputTokens) > 0));
  assert.ok(metrics.some((metric) => Number(metric.cachedInputTokens) > 0));

  const evidence = {
    schemaVersion: "task-6530-live-v1",
    generatedAt: new Date().toISOString(),
    baseUrl,
    serviceUnit,
    provider: {
      alias: providerAlias,
      modelId: provider.modelId,
      providerKind: provider.providerKind,
      protocol: provider.protocol,
      responsesDialect: provider.responsesDialect,
      credential: "[redacted]",
    },
    profileId,
    sessionId,
    restartProof,
    scenarios: {
      simple: summarize(simple),
      continuity: summarize(continuity),
      sequential: summarize(sequential),
      parallel: summarize(parallel),
      recovery: summarize(recovery),
      ...(restart === undefined ? {} : { restart: summarize(restart) }),
      long: summarize(long),
    },
    exactProviderRequestCount: requests.length,
    requestContract: {
      fullReplayObserved: requests.some(
        (request) => inputItems(request).length > 4,
      ),
      forbiddenFieldsAbsent: true,
    },
    usage: metrics.map((metric) => ({
      wakeId: metric.wakeId,
      inputTokens: metric.inputTokens,
      cachedInputTokens: metric.cachedInputTokens,
      outputTokens: metric.outputTokens,
      reasoningOutputTokens: metric.reasoningOutputTokens,
      totalTokens: metric.totalTokens,
    })),
  };
  const evidenceDirectory = `${evidenceRoot}/${suffix}`;
  await mkdir(evidenceDirectory, { recursive: true });
  await writeFile(
    `${evidenceDirectory}/live-provider-results.json`,
    `${JSON.stringify(evidence, null, 2)}\n`,
    "utf8",
  );
  console.log(JSON.stringify({ ...evidence, evidenceDirectory }, null, 2));
} finally {
  if (!keepProfile && sessionId !== undefined) {
    await api(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      {
        confirmProfileId: profileId,
        reason: "task 6530 live certification cleanup",
      },
    ).catch(() => undefined);
  }
}

interface ChatEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
  payload: Record<string, unknown>;
}

interface ScenarioResult {
  cursor: string;
  events: ChatEvent[];
}

async function sendAndWait(
  currentSessionId: string,
  body: string,
  initialCursor?: string,
): Promise<ScenarioResult> {
  const cursor = initialCursor ?? (await latestCursor(currentSessionId));
  const key = `task-6530:${currentSessionId}:${Date.now()}:${Math.random()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/messages`,
    {
      actor: { id: "task-6530-certifier", kind: "human" },
      body,
      client_message_id: key,
      reason: "task 6530 DeepSeek Responses live certification",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  return waitForEvents(currentSessionId, cursor);
}

async function sendAndRestart(
  currentSessionId: string,
  cursor: string,
  body: string,
): Promise<ScenarioResult> {
  const key = `task-6530-restart:${currentSessionId}:${Date.now()}`;
  const submission = api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/messages`,
    {
      actor: { id: "task-6530-certifier", kind: "human" },
      body,
      client_message_id: key,
      reason: "task 6530 restart-safe continuation certification",
    },
    { "Idempotency-Key": key },
  ).then(
    (response) => ({ response }),
    (error: unknown) => ({ error }),
  );
  await waitForEventKind(
    currentSessionId,
    cursor,
    "logical_turn_queued_to_continue",
  );
  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
  await waitForService();
  const sent = await submission;
  if ("response" in sent)
    assert.equal(sent.response.status, 202, sent.response.text);
  return waitForEvents(currentSessionId, cursor);
}

async function waitForEvents(
  currentSessionId: string,
  initialCursor: string,
): Promise<ScenarioResult> {
  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 600_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    for (const event of nestedArray(page, "items") as unknown as ChatEvent[]) {
      if (!events.some((seen) => seen.event_id === event.event_id))
        events.push(event);
    }
    cursor =
      typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    const terminal = events.find(
      (event) => event.kind === "assistant_turn_finished",
    );
    if (terminal) {
      assert.notEqual(
        terminal.payload.status,
        "failed",
        JSON.stringify(events, null, 2),
      );
      assert.ok(
        events.some(
          (event) =>
            event.kind === "assistant_message_completed" &&
            event.payload.status === "completed",
        ),
        JSON.stringify(events, null, 2),
      );
      return { cursor, events };
    }
    await delay(100);
  }
  throw new Error(`timed out waiting for ${currentSessionId}`);
}

async function waitForEventKind(
  currentSessionId: string,
  initialCursor: string,
  kind: string,
): Promise<void> {
  let cursor = initialCursor;
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    try {
      const page = await apiData<Record<string, unknown>>(
        "GET",
        `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
      );
      const events = nestedArray(page, "items") as unknown as ChatEvent[];
      if (events.some((event) => event.kind === kind)) return;
      cursor =
        typeof page.latest_cursor === "string" ? page.latest_cursor : cursor;
    } catch {
      // The planned restart window is retryable.
    }
    await delay(50);
  }
  throw new Error(`timed out waiting for ${kind}`);
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      if (
        (await api("GET", "/v1/admin/healthz", undefined, {}, 5_000)).status ===
        200
      )
        return;
    } catch {
      // Expected while systemd is restarting.
    }
    await delay(250);
  }
  throw new Error(`${serviceUnit} did not become healthy`);
}

async function latestCursor(currentSessionId: string): Promise<string> {
  const sessions = await apiData<Record<string, unknown>>(
    "GET",
    "/v1/chat/sessions?limit=500",
  );
  const session = nestedArray(sessions, "items").find(
    (candidate) => candidate.session_id === currentSessionId,
  );
  assert.ok(session, `chat inventory must contain ${currentSessionId}`);
  return String(session.latest_cursor ?? `${currentSessionId}:0`);
}

async function profileDiagnostic(
  currentProfileId: string,
): Promise<Record<string, unknown>> {
  const diagnostics = await apiData<unknown[]>(
    "GET",
    "/v1/admin/diagnostics/provider-state",
  );
  const diagnostic = diagnostics.find(
    (candidate) => nested(candidate, "profileId") === currentProfileId,
  );
  assert.ok(
    diagnostic,
    `provider-state diagnostics missing ${currentProfileId}`,
  );
  return diagnostic as Record<string, unknown>;
}

async function exactProviderDebugDetails(
  currentSessionId: string,
  events: ChatEvent[],
): Promise<Record<string, unknown>[]> {
  const details: Record<string, unknown>[] = [];
  for (const id of providerDebugIds(events)) {
    const response = await api(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(currentSessionId)}/provider-requests/${encodeURIComponent(id)}`,
    );
    if (response.status === 404 && restartProof) continue;
    assert.ok(response.status < 400, response.text);
    assert.equal(response.json.ok, true, response.text);
    const detail = response.json.data as Record<string, unknown>;
    if (
      nested(detail, "request", "value", "boundary") ===
      "rust_openai_responses_request"
    ) {
      assert.equal(JSON.stringify(detail).includes("Bearer "), false);
      details.push(detail);
    }
  }
  return details;
}

function providerDebugIds(events: ChatEvent[]): string[] {
  const ids = new Set<string>();
  for (const event of events) {
    for (const candidate of [
      event.payload.metadata,
      event.payload.metadataJson,
      event.payload.metadata_json,
    ]) {
      const metadata = parseObject(candidate);
      const id = metadata?.provider_request_debug_detail_id;
      if (typeof id === "string") ids.add(id);
    }
  }
  return [...ids];
}

function providerRequests(
  detail: Record<string, unknown>,
): Record<string, unknown>[] {
  return nestedArray(detail, "request", "value", "requests");
}

function assertDeepseekRequest(request: Record<string, unknown>): void {
  assert.equal(request.model, "deepseek-v4-flash");
  assert.equal(request.stream, true);
  for (const field of [
    "previous_response_id",
    "conversation",
    "store",
    "background",
    "include",
    "service_tier",
    "prompt_cache_key",
    "prompt_cache_retention",
    "parallel_tool_calls",
    "text",
  ]) {
    assert.equal(
      field in request,
      false,
      `DeepSeek request contained ${field}`,
    );
  }
  assert.ok(Array.isArray(request.input));
}

function inputItems(request: Record<string, unknown>): unknown[] {
  return Array.isArray(request.input) ? request.input : [];
}

function assistantText(events: ChatEvent[]): string {
  return events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload.text ?? ""))
    .join("");
}

function successfulTools(
  events: ChatEvent[],
): Array<{ callId: string; name: string }> {
  return events
    .filter(
      (event) =>
        event.kind === "tool_call_completed" && event.payload.is_error !== true,
    )
    .map((event) => ({
      callId: String(event.payload.tool_call_id ?? event.payload.call_id ?? ""),
      name: String(event.payload.tool_name ?? ""),
    }));
}

function summarize(result: ScenarioResult): Record<string, unknown> {
  return {
    latestCursor: result.cursor,
    eventCount: result.events.length,
    wakeIds: [
      ...new Set(
        result.events
          .map((event) => event.payload.wake_id)
          .filter((value): value is string => typeof value === "string"),
      ),
    ],
    reasoningDeltaCount: result.events.filter(
      (event) => event.kind === "assistant_reasoning_delta",
    ).length,
    tools: result.events
      .filter((event) => event.kind === "tool_call_completed")
      .map((event) => ({
        name: event.payload.tool_name,
        isError: event.payload.is_error === true,
      })),
  };
}

function wakeIds(events: ChatEvent[]): string[] {
  return [
    ...new Set(
      events
        .map((event) => event.payload.wake_id)
        .filter((value): value is string => typeof value === "string"),
    ),
  ];
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

function nestedArray(
  value: unknown,
  ...path: string[]
): Record<string, unknown>[] {
  const result = nested(value, ...path);
  return Array.isArray(result) ? (result as Record<string, unknown>[]) : [];
}

function nestedString(value: unknown, ...path: string[]): string | undefined {
  const result = nested(value, ...path);
  return typeof result === "string" ? result : undefined;
}

async function apiData<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await api(method, path, body);
  assert.ok(response.status < 400, response.text);
  assert.equal(response.json.ok, true, response.text);
  return response.json.data as T;
}

async function api(
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
  timeoutMs = 600_000,
) {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(timeoutMs),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  try {
    json = JSON.parse(text) as Record<string, unknown>;
  } catch {
    // Assertions retain raw response bodies.
  }
  return { status: response.status, text, json };
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
