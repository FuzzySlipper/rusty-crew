import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";

const baseUrl = new URL(
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348",
);
if (
  process.env.RUSTY_CREW_ALLOW_NONDEBUG_COMMAND_LIVE !== "1" &&
  baseUrl.port !== "9348"
) {
  throw new Error(
    `external command live smoke refuses non-debug service ${baseUrl.origin}`,
  );
}

const token = process.env.RUSTY_CREW_ADMIN_TOKEN;
const requestedBindingId = process.env.RUSTY_CREW_EXTERNAL_COMMAND_BINDING_ID;
const runId = `task-5739-${Date.now()}-${randomUUID().slice(0, 8)}`;

const bindings = await apiGet<{
  bindings: Array<{
    bindingId: string;
    runtimeId: string;
    nativeThreadId?: string;
    status: string;
    updatedAt: string;
  }>;
}>("/v1/external-bindings");
const binding = bindings.bindings
  .filter(
    (candidate) =>
      candidate.status === "active" &&
      typeof candidate.nativeThreadId === "string" &&
      (requestedBindingId === undefined ||
        candidate.bindingId === requestedBindingId),
  )
  .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt))[0];
assert.ok(
  binding?.nativeThreadId,
  requestedBindingId === undefined
    ? "debug service has no active external binding with a native thread"
    : `debug service has no active external binding ${requestedBindingId}`,
);

const initialCatalog = await apiGet<CommandCatalog>(
  `/v1/external-bindings/${encodeURIComponent(binding.bindingId)}/commands`,
);
assert.ok(initialCatalog.commands.some((command) => command.name === "status"));
assert.ok(
  initialCatalog.models.length > 0,
  "live model/list returned no models",
);
const before = await readThread(binding.runtimeId, binding.nativeThreadId);
const eventCursor = await latestEventCursor(binding.runtimeId);

const status = await command(binding.bindingId, "/status", `${runId}:status`);
assert.equal(status.status, "applied");
assert.equal(status.result.status?.nativeThreadId, binding.nativeThreadId);
const afterStatus = await readThread(binding.runtimeId, binding.nativeThreadId);
assert.deepEqual(
  afterStatus.thread.turns,
  before.thread.turns,
  "/status changed the native turn transcript",
);

const originalSettings = initialCatalog.settings;
const selectedModel =
  initialCatalog.models.find(
    (model) =>
      model.model !== originalSettings.model &&
      model.supportedEfforts.length > 0,
  ) ?? initialCatalog.models.find((model) => model.supportedEfforts.length > 0);
assert.ok(selectedModel, "live model catalog has no model with effort options");
const selectedEffort = selectedModel.supportedEfforts[0];
assert.ok(selectedEffort);

const modelResult = await command(
  binding.bindingId,
  `/model ${selectedModel.id}`,
  `${runId}:model`,
);
assert.equal(modelResult.status, "applied");
assert.equal(modelResult.result.settings?.model, selectedModel.model);

const effortResult = await command(
  binding.bindingId,
  `/effort ${selectedEffort.value}`,
  `${runId}:effort`,
);
assert.equal(effortResult.status, "applied");
assert.equal(effortResult.result.settings?.effort, selectedEffort.value);

const invalidEffort = await command(
  binding.bindingId,
  "/effort definitely-not-advertised",
  `${runId}:invalid-effort`,
);
assert.equal(invalidEffort.status, "rejected");
assert.equal(invalidEffort.reasonCode, "external_command_effort_invalid");

const readback = await apiGet<CommandCatalog>(
  `/v1/external-bindings/${encodeURIComponent(binding.bindingId)}/commands`,
);
assert.equal(readback.settings.model, selectedModel.model);
assert.equal(readback.settings.effort, selectedEffort.value);
const afterCommands = await readThread(
  binding.runtimeId,
  binding.nativeThreadId,
);
assert.deepEqual(
  afterCommands.thread.turns,
  before.thread.turns,
  "external commands changed the native turn transcript",
);

const liveTurn = await deliverLiveTurn(
  binding.bindingId,
  `${runId}:selected-settings-turn`,
);
assert.equal(liveTurn.phase, "completed");
assert.equal(liveTurn.nativeThreadId, binding.nativeThreadId);
const settingsAfterTurn = await apiGet<CommandCatalog>(
  `/v1/external-bindings/${encodeURIComponent(binding.bindingId)}/commands`,
);
assert.equal(settingsAfterTurn.settings.model, selectedModel.model);
assert.equal(settingsAfterTurn.settings.effort, selectedEffort.value);

const compact = await command(
  binding.bindingId,
  "/compact",
  `${runId}:compact`,
);
assert.equal(
  compact.status,
  "applied",
  `native compact was not applied: ${compact.reasonCode ?? compact.message}`,
);

await restoreSettings(binding.bindingId, originalSettings, runId);
const events = await waitForEvents(binding.runtimeId, eventCursor, [
  `${status.commandId}:command_started`,
  `${status.commandId}:command_completed`,
  `${invalidEffort.commandId}:command_failed`,
  `${compact.commandId}:command_completed`,
]);
assert.ok(
  events.some(
    (event) =>
      event.nativeThreadId === binding.nativeThreadId &&
      (event.kind === "compaction" ||
        event.payload.nativeMethod === "thread/compacted"),
  ),
  "native compaction evidence did not reach external-runtime replay",
);
assert.ok(
  events.some(
    (event) =>
      event.nativeThreadId === binding.nativeThreadId &&
      event.payload.nativeMethod === "thread/settings/updated",
  ),
  "native settings update did not reach external-runtime replay",
);

console.log(
  JSON.stringify({
    baseUrl: baseUrl.origin,
    bindingId: binding.bindingId,
    runtimeId: binding.runtimeId,
    nativeThreadId: binding.nativeThreadId,
    modelCount: initialCatalog.models.length,
    selectedModel: selectedModel.model,
    selectedEffort: selectedEffort.value,
    selectedSettingsTurnId: liveTurn.nativeTurnId,
    statusCommandId: status.commandId,
    compactCommandId: compact.commandId,
    replayedEvents: events.length,
    nativeTurnsBeforeStatus: before.thread.turns.length,
    nativeTurnsAfterStatus: afterStatus.thread.turns.length,
    nativeTurnsAfterCommands: afterCommands.thread.turns.length,
  }),
);

async function deliverLiveTurn(
  bindingId: string,
  suffix: string,
): Promise<ExternalTurn> {
  const delivery = await apiPost<{
    activation?: { type: string; requestId?: string };
  }>(`/v1/external-bindings/${encodeURIComponent(bindingId)}/messages`, {
    deliveryId: `${suffix}:delivery`,
    idempotencyKey: `${suffix}:delivery`,
    messageId: `${suffix}:message`,
    body: "Reply with exactly EXTERNAL_COMMAND_SETTINGS_OK and nothing else.",
    ttlMs: 60_000,
  });
  assert.equal(delivery.activation?.type, "external_turn_requested");
  assert.ok(delivery.activation.requestId);
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    const turn = await apiGet<ExternalTurn>(
      `/v1/external-turns/${encodeURIComponent(delivery.activation.requestId)}`,
    );
    if (
      turn.phase === "completed" ||
      turn.phase === "failed" ||
      turn.phase === "interrupted" ||
      turn.phase === "outcome_unknown"
    ) {
      return turn;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error("timed out waiting for selected-settings live turn");
}

async function restoreSettings(
  bindingId: string,
  settings: CommandCatalog["settings"],
  suffix: string,
): Promise<void> {
  const catalog = await apiGet<CommandCatalog>(
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/commands`,
  );
  const originalModel = catalog.models.find(
    (model) => model.model === settings.model || model.id === settings.model,
  );
  if (originalModel === undefined) return;
  const model = await command(
    bindingId,
    `/model ${originalModel.id}`,
    `${suffix}:restore-model`,
  );
  assert.equal(model.status, "applied");
  if (
    settings.effort !== null &&
    originalModel.supportedEfforts.some(
      (effort) => effort.value === settings.effort,
    )
  ) {
    const effort = await command(
      bindingId,
      `/effort ${settings.effort}`,
      `${suffix}:restore-effort`,
    );
    assert.equal(effort.status, "applied");
  }
}

async function command(
  bindingId: string,
  input: string,
  idempotencyKey: string,
): Promise<CommandExecution> {
  return apiPost(
    `/v1/external-bindings/${encodeURIComponent(bindingId)}/commands`,
    { input, idempotencyKey },
  );
}

async function readThread(
  runtimeId: string,
  threadId: string,
): Promise<{
  thread: { turns: unknown[] };
}> {
  return apiPost(
    `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/threads/read`,
    { threadId, includeTurns: true },
  );
}

async function latestEventCursor(runtimeId: string): Promise<number> {
  let cursor = 0;
  while (true) {
    const page = await apiGet<{ events: ExternalEvent[] }>(
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/events?after=${cursor}&limit=1000`,
    );
    if (page.events.length === 0) return cursor;
    cursor = page.events.at(-1)?.sequenceId ?? cursor;
    if (page.events.length < 1000) return cursor;
  }
}

async function waitForEvents(
  runtimeId: string,
  after: number,
  requiredEventIds: readonly string[],
): Promise<ExternalEvent[]> {
  const events: ExternalEvent[] = [];
  const deadline = Date.now() + 60_000;
  let cursor = after;
  while (Date.now() < deadline) {
    const page = await apiGet<{ events: ExternalEvent[] }>(
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/events?after=${cursor}&limit=1000`,
    );
    events.push(...page.events);
    cursor = page.events.at(-1)?.sequenceId ?? cursor;
    if (
      requiredEventIds.every((id) =>
        events.some((event) => event.eventId === id),
      )
    ) {
      return events;
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(
    `timed out waiting for command events: ${requiredEventIds
      .filter((id) => !events.some((event) => event.eventId === id))
      .join(", ")}`,
  );
}

async function apiGet<T>(path: string): Promise<T> {
  return api<T>(path, { method: "GET" });
}

async function apiPost<T>(path: string, body: unknown): Promise<T> {
  return api<T>(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

async function api<T>(path: string, init: RequestInit): Promise<T> {
  const headers = new Headers(init.headers);
  if (token !== undefined && token !== "") {
    headers.set("authorization", `Bearer ${token}`);
  }
  const response = await fetch(new URL(path, baseUrl), { ...init, headers });
  const payload = (await response.json()) as ApiEnvelope<T>;
  if (!response.ok || payload.ok !== true) {
    throw new Error(
      `Crew ${init.method ?? "GET"} ${path} failed (${response.status}): ${JSON.stringify(payload)}`,
    );
  }
  return payload.data;
}

interface ApiEnvelope<T> {
  ok: boolean;
  data: T;
  error?: unknown;
}

interface CommandCatalog {
  settings: { model: string; modelProvider: string; effort: string | null };
  commands: Array<{ name: string }>;
  models: Array<{
    id: string;
    model: string;
    supportedEfforts: Array<{ value: string; description: string }>;
  }>;
}

interface CommandExecution {
  commandId: string;
  status: "pending" | "applied" | "rejected" | "failed";
  reasonCode: string | null;
  message: string;
  result: {
    status?: {
      nativeThreadId: string;
      usage: unknown;
    };
    settings?: { model: string; modelProvider: string; effort: string | null };
  };
}

interface ExternalEvent {
  eventId: string;
  sequenceId: number;
  kind: string;
  nativeThreadId?: string;
  payload: { nativeMethod?: string };
}

interface ExternalTurn {
  phase: string;
  nativeTurnId?: string;
  nativeThreadId?: string;
}
