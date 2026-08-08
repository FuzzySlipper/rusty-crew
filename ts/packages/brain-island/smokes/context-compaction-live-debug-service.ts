import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, writeFile } from "node:fs/promises";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/+$/, "");
const serviceUnit =
  process.env.RUSTY_CREW_DEBUG_SERVICE_UNIT ?? "rusty-crew-debug.service";
const providerBaseUrl =
  process.env.RUSTY_CREW_CONTEXT_CERT_PROVIDER_BASE_URL ??
  "http://127.0.0.1:18082/v1";
const modelId =
  process.env.RUSTY_CREW_CONTEXT_CERT_MODEL_ID ?? "deepseek-flash";
const evidenceRoot =
  process.env.RUSTY_CREW_CONTEXT_CERT_EVIDENCE_ROOT ??
  "/home/system/rusty-crew-debug/evidence/task-6617";
const expectedSourceRevision =
  process.env.RUSTY_CREW_CONTEXT_CERT_SOURCE_SHA ??
  execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim();
const suffix = Date.now().toString(36);
const successfulCompactAtPercent = 60;
const successfulTargetPercentAfterCompaction = 30;
const providerAlias = `task-6617-context-${suffix}`;
const failureProviderAlias = `task-6617-context-failure-${suffix}`;
const profileId = `task-6617-context-${suffix}`;
const failureProfileId = `task-6617-context-failure-${suffix}`;
let providerCreated = false;
let failureProviderCreated = false;
let profileCreated = false;
let failureProfileCreated = false;

assert.equal(
  new URL(baseUrl).port,
  "9348",
  "context certification is debug-only",
);
assert.equal(serviceUnit, "rusty-crew-debug.service");

try {
  await assertServiceSourceRevision();
  await createProvider(providerAlias, 16_384, 512, {
    certification: "task-6617",
    scenario: "successful-compaction",
  });
  providerCreated = true;
  const successful = await createProfile(profileId, providerAlias);
  profileCreated = true;
  await applyContextPolicy(profileId, successful.profileRevision, {
    enabled: true,
    strategyId: "recent_window",
    autoCompactionEnabled: true,
    compactAtPercent: successfulCompactAtPercent,
    targetPercentAfterCompaction: successfulTargetPercentAfterCompaction,
    maxContextPercentForWake: 95,
    debugVisibility: "status",
    includeDebugEventsInModelContext: false,
    strategyConfig: { certification: "task-6617" },
  });

  const snapshots: Record<string, unknown>[] = [];
  const turnResults: TurnResult[] = [];
  let cursor = await latestCursor(successful.sessionId);
  for (let turn = 1; turn <= 20; turn += 1) {
    const marker = `CONTEXT_CERT_TOOL_${turn}_${suffix}`;
    const body = [
      turn === 1
        ? `Remember this exact continuity fact: CONTEXT_FACT_${suffix}.`
        : "Preserve the exact continuity fact from the earlier turns.",
      `Use terminal exactly once to run printf '${marker}'.`,
      `After the tool result, reply with ${marker} and one short sentence.`,
      "Do not call any other tool.",
    ].join("\n");
    const result = await sendAndWait(successful.sessionId, body, cursor);
    turnResults.push(result);
    cursor = result.cursor;
    const snapshot = await readNativeSnapshot(successful.sessionId);
    snapshots.push(snapshot);
    assertSnapshot(snapshot);
    assert.ok(
      successfulToolCount(result.events) >= 1,
      `turn ${turn} must complete a real terminal tool call`,
    );
    if (hasCompletedCompaction(snapshot, result.events)) break;
  }

  const successfulEvents = orderEvents(
    await readAllEvents(successful.sessionId),
  );
  const successfulSnapshots = [
    ...snapshots,
    ...snapshotsFromEvents(successfulEvents),
  ];
  const beforePressure = successfulSnapshots.find(
    (snapshot) =>
      snapshot.compaction?.lastArtifactId === null &&
      typeof snapshot.admission?.fillPercent === "number" &&
      snapshot.admission.fillPercent < successfulCompactAtPercent,
  );
  const duringPressureSnapshot = successfulSnapshots.find(
    (snapshot) =>
      snapshot.compaction?.lastArtifactId === null &&
      (snapshot.admission?.state === "requires_compaction" ||
        (typeof snapshot.admission?.fillPercent === "number" &&
          snapshot.admission.fillPercent >= successfulCompactAtPercent)),
  );
  assert.ok(beforePressure, "missing pre-pressure native context snapshot");
  const compactionStarted = successfulEvents
    .flatMap(eventMetadata)
    .find((metadata) => metadata.kind === "context_compaction_started");
  assert.ok(
    compactionStarted,
    "live event log must contain context_compaction_started",
  );
  assert.ok(
    isRecord(compactionStarted.usage),
    "compaction started event must carry the pressure accounting usage",
  );
  const afterCompaction = firstSnapshotAfterMetadata(
    successfulEvents,
    "context_compaction_completed",
  );
  assert.ok(
    afterCompaction,
    "live event log must contain an accounting snapshot after context_compaction_completed",
  );
  assert.equal(
    afterCompaction.promptProjection?.inputTokens?.source,
    "provider",
    "post-compaction request must carry provider input-token provenance",
  );
  assert.equal(
    afterCompaction.promptProjection?.inputTokens?.quality,
    "exact",
    "post-compaction request must carry exact input-token quality",
  );
  assert.equal(
    afterCompaction.providerUsage?.currentRequest?.inputTokens?.source,
    "provider",
    "post-compaction current request must be provider-reported",
  );
  assert.equal(
    afterCompaction.providerUsage?.currentRequest?.inputTokens?.quality,
    "exact",
    "post-compaction current request must be exact",
  );
  assert.notEqual(
    afterCompaction.admission?.state,
    "requires_compaction",
    "the request immediately after compaction must be admitted",
  );
  assert.ok(
    Number(afterCompaction.admission?.fillPercent) < successfulCompactAtPercent,
    "post-compaction provider request must be below the compaction threshold",
  );
  assert.ok(
    Number(afterCompaction.promptProjection?.inputTokens?.tokens) <
      Number(compactionStarted.usage.inputTokens),
    "post-compaction provider request must be smaller than the pressured request",
  );
  const firstTurnSummary = assistantSummary(turnResults[0]?.events ?? []);
  assert.match(firstTurnSummary, new RegExp(`CONTEXT_CERT_TOOL_1_${suffix}`));

  const hydratedBeforeRestart = await readNativeSnapshot(successful.sessionId);
  const artifactIdBeforeRestart =
    hydratedBeforeRestart.compaction?.lastArtifactId;
  assert.ok(
    artifactIdBeforeRestart,
    "compaction lineage must be present before restart",
  );
  execFileSync("systemctl", ["--user", "restart", serviceUnit], {
    stdio: "inherit",
  });
  await waitForService();
  const hydratedAfter = await readNativeSnapshot(successful.sessionId);
  const durableCountBeforeRestart = durableMessageCount(
    hydratedBeforeRestart,
    "before restart",
  );
  const durableCountAfterCompaction = durableMessageCount(
    afterCompaction,
    "after compaction",
  );
  const durableCountAfterRestart = durableMessageCount(
    hydratedAfter,
    "after restart hydration",
  );
  assert.ok(
    durableCountBeforeRestart >= durableCountAfterCompaction,
    "durable transcript must not shrink between compaction and pre-restart hydration",
  );
  assert.equal(hydratedAfter.sessionId, successful.sessionId);
  assert.equal(
    hydratedAfter.compaction?.lastArtifactId,
    artifactIdBeforeRestart,
  );
  assert.ok(
    durableCountAfterRestart >= durableCountBeforeRestart,
    "restart must not discard the durable transcript",
  );
  await assertServiceSourceRevision();
  const restarted = await sendAndWait(
    successful.sessionId,
    `After the debug service restart, recall CONTEXT_FACT_${suffix}, then use terminal exactly once to run printf 'CONTEXT_RESTART_CONTINUITY_${suffix}'. Reply with both the remembered fact and the exact terminal marker.`,
    await latestCursor(successful.sessionId),
  );
  assert.equal(
    assistantSummary(restarted.events).includes(`CONTEXT_FACT_${suffix}`),
    true,
    "restart must retain a pre-compaction continuity fact",
  );
  assert.equal(
    assistantSummary(restarted.events).includes(
      `CONTEXT_RESTART_CONTINUITY_${suffix}`,
    ),
    true,
    "same session must continue with the real provider after restart",
  );
  assert.ok(successfulToolCount(restarted.events) >= 1);
  const restartedSnapshot = await readNativeSnapshot(successful.sessionId);
  assertSnapshot(restartedSnapshot);
  const durableCountAfterContinuation = durableMessageCount(
    restartedSnapshot,
    "after restart continuation",
  );
  assert.ok(
    durableCountAfterContinuation >= durableCountAfterRestart,
    "a subsequent turn and compaction must not shrink the durable transcript",
  );
  assert.notEqual(
    restartedSnapshot.admission?.state,
    "requires_compaction",
    `the first provider request after restart must be admitted: ${JSON.stringify(
      {
        snapshot: summarizeSnapshot(restartedSnapshot),
        events: orderEvents(restarted.events).map((event) => ({
          sequenceId: event.sequence_id,
          kind: event.kind,
          wakeId: nestedString(event, "payload", "wake_id"),
          metadataKinds: eventMetadataKinds(event),
        })),
      },
    )}`,
  );
  assert.ok(
    Number(restartedSnapshot.promptProjection?.inputTokens?.tokens) <
      Number(compactionStarted.usage.inputTokens),
    "the first provider request after restart must retain the reduced projection",
  );

  await createProvider(failureProviderAlias, 12_288, 512, {
    certification: "task-6617",
    scenario: "failed-compaction-preserves-state",
  });
  failureProviderCreated = true;
  const failure = await createProfile(failureProfileId, failureProviderAlias);
  failureProfileCreated = true;
  await applyContextPolicy(failureProfileId, failure.profileRevision, {
    enabled: true,
    strategyId: "recent_window",
    autoCompactionEnabled: true,
    compactAtPercent: 25,
    targetPercentAfterCompaction: 12,
    maxContextPercentForWake: 95,
    debugVisibility: "status",
    includeDebugEventsInModelContext: false,
    strategyConfig: { certification: "task-6617", expectedFailure: true },
  });
  const failedTurn = await sendAndWait(
    failure.sessionId,
    `Use terminal exactly once to run printf 'CONTEXT_FAILURE_PRESERVATION_${suffix}'. Then answer with the marker.`,
    await latestCursor(failure.sessionId),
  );
  const failedEvents = await readAllEvents(failure.sessionId);
  assert.ok(
    failedEvents.some((event) =>
      eventMetadataKinds(event).includes("context_compaction_failed"),
    ),
    `failure scenario must emit context_compaction_failed: ${JSON.stringify(failedTurn.events)}`,
  );
  assert.ok(
    failedEvents.some(
      (event) =>
        event.kind === "logical_turn_attention_required" ||
        event.kind === "logical_turn_failed",
    ),
    "failed compaction must surface recoverable attention or a terminal failure",
  );
  const failedSnapshot = await readNativeSnapshot(failure.sessionId);
  assertSnapshot(failedSnapshot);
  assert.ok(
    Number(failedSnapshot.durableTranscript?.messageCount) > 0,
    "failed compaction must preserve durable transcript state",
  );
  assert.equal(
    failedSnapshot.providerState?.stateKind,
    "chat_completions_messages",
    "failed compaction must preserve the prior provider projection",
  );

  const evidenceDirectory = `${evidenceRoot}/${suffix}`;
  await mkdir(evidenceDirectory, { recursive: true });
  const evidence = {
    schemaVersion: "task-6617-live-v1",
    generatedAt: new Date().toISOString(),
    service: {
      baseUrl,
      serviceUnit,
      providerBaseUrl,
      modelId,
      sourceRevision: expectedSourceRevision,
    },
    successfulScenario: {
      providerAlias,
      profileId,
      sessionId: successful.sessionId,
      turnCount: turnResults.length,
      successfulToolCalls: turnResults.reduce(
        (total, result) => total + successfulToolCount(result.events),
        0,
      ),
      beforePressure: summarizeSnapshot(beforePressure),
      duringPressure: {
        snapshot:
          duringPressureSnapshot === undefined
            ? null
            : summarizeSnapshot(duringPressureSnapshot),
        startedEventUsage: compactionStarted.usage,
      },
      afterCompaction: summarizeSnapshot(afterCompaction),
      compactionKinds: [
        ...new Set(successfulEvents.flatMap(eventMetadataKinds)),
      ],
      artifactId: artifactIdBeforeRestart,
      eventRefs: successfulEvents.map(eventReference),
      durableTranscriptCounts: {
        afterCompaction: durableCountAfterCompaction,
        beforeRestart: durableCountBeforeRestart,
        afterRestartHydration: durableCountAfterRestart,
        afterRestartContinuation: durableCountAfterContinuation,
      },
      hydratedBeforeRestart: summarizeSnapshot(hydratedBeforeRestart),
      hydratedAfterRestart: summarizeSnapshot(hydratedAfter),
      restartContinuity: summarizeSnapshot(restartedSnapshot),
      restartedToolCalls: successfulToolCount(restarted.events),
    },
    failureScenario: {
      providerAlias: failureProviderAlias,
      profileId: failureProfileId,
      sessionId: failure.sessionId,
      compactionKinds: [...new Set(failedEvents.flatMap(eventMetadataKinds))],
      eventRefs: failedEvents.map(eventReference),
      snapshot: summarizeSnapshot(failedSnapshot),
    },
  };
  await writeFile(
    `${evidenceDirectory}/live-results.json`,
    `${JSON.stringify(evidence, null, 2)}\n`,
    "utf8",
  );
  console.log(JSON.stringify({ ...evidence, evidenceDirectory }, null, 2));
} finally {
  if (failureProfileCreated) {
    await deleteProfile(failureProfileId).catch((error) =>
      console.error(`failure profile cleanup failed: ${errorMessage(error)}`),
    );
  }
  if (profileCreated) {
    await deleteProfile(profileId).catch((error) =>
      console.error(`profile cleanup failed: ${errorMessage(error)}`),
    );
  }
  if (failureProviderCreated) {
    await archiveProvider(failureProviderAlias).catch((error) =>
      console.error(`failure provider cleanup failed: ${errorMessage(error)}`),
    );
  }
  if (providerCreated) {
    await archiveProvider(providerAlias).catch((error) =>
      console.error(`provider cleanup failed: ${errorMessage(error)}`),
    );
  }
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

interface ChatEvent {
  event_id?: string;
  sequence_id?: number;
  kind?: string;
  payload?: Record<string, unknown>;
}

interface TurnResult {
  cursor: string;
  events: ChatEvent[];
}

async function createProvider(
  alias: string,
  contextWindowTokens: number,
  maxOutputTokens: number,
  metadataJson: Record<string, unknown>,
): Promise<void> {
  const result = await api("POST", "/v1/admin/model-providers", {
    alias,
    status: "active",
    protocol: "chat_completions",
    providerKind: "custom",
    displayName: alias,
    baseUrl: providerBaseUrl,
    modelId,
    contextWindowTokens,
    maxOutputTokens,
    reasoningEffort: "low",
    chatCompletionsDialect: "deepseek",
    thinkingMode: "provider_default",
    reasoningHistory: "provider_default",
    promptCaching: "disabled",
    metadataJson,
  });
  assert.equal(result.status, 200, result.text);
  assert.equal(result.json.ok, true, result.text);
  assert.equal(
    nested(result.json, "data", "provider", "credential", "hasSecret"),
    false,
  );
}

async function archiveProvider(alias: string): Promise<void> {
  const current = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/model-providers/${encodeURIComponent(alias)}`,
  );
  const result = await api(
    "PATCH",
    `/v1/admin/model-providers/${encodeURIComponent(alias)}?refresh=apply`,
    providerWriteBody(current, {
      status: "archived",
      expectedRevision: current.revision,
    }),
  );
  assert.ok(result.status < 400, result.text);
  assert.equal(result.json.ok, true, result.text);
}

async function createProfile(
  currentProfileId: string,
  alias: string,
): Promise<{ sessionId: string; profileRevision: number }> {
  const created = await apiData<Record<string, unknown>>(
    "POST",
    "/v1/admin/control/profiles",
    {
      profileId: currentProfileId,
      displayName: `Task 6617 context certification ${suffix}`,
      providerAlias: alias,
      kind: "full",
      workspaceCwd: "/home/dev/rusty-crew",
      localToolProfileId: "full_coding_agent",
      reason: "task 6617 live context compaction certification",
    },
  );
  const sessionId = nestedString(created, "outcome", "result", "sessionId");
  assert.ok(
    sessionId,
    `profile creation did not return a session: ${JSON.stringify(created)}`,
  );
  const registry = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/admin/profiles/registry/${encodeURIComponent(currentProfileId)}`,
  );
  const revision = registry.revision;
  assert.equal(typeof revision, "number");
  return { sessionId, profileRevision: revision as number };
}

async function applyContextPolicy(
  currentProfileId: string,
  expectedRevision: number,
  contextPolicy: Record<string, unknown>,
): Promise<void> {
  const result = await api(
    "POST",
    `/v1/admin/profiles/registry/${encodeURIComponent(currentProfileId)}/runtime-config/apply`,
    {
      expectedRevision,
      contextPolicy,
      localToolProfileId: "full_coding_agent",
      mcpBindings: [],
    },
  );
  assert.equal(result.status, 200, result.text);
  assert.equal(result.json.ok, true, result.text);
  assert.equal(nested(result.json, "data", "applied"), true, result.text);
}

async function deleteProfile(currentProfileId: string): Promise<void> {
  const result = await api(
    "POST",
    `/v1/admin/control/profiles/${encodeURIComponent(currentProfileId)}/delete`,
    {
      confirmProfileId: currentProfileId,
      reason: "task 6617 live certification cleanup",
    },
  );
  assert.ok(result.status < 400, result.text);
}

async function sendAndWait(
  sessionId: string,
  body: string,
  initialCursor: string,
): Promise<TurnResult> {
  const key = `task-6617:${sessionId}:${Date.now()}:${Math.random()}`;
  const sent = await api(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "task-6617-certifier", kind: "human" },
      body,
      client_message_id: key,
      reason: "task 6617 live context compaction certification",
    },
    { "Idempotency-Key": key },
  );
  assert.equal(sent.status, 202, sent.text);
  let cursor = initialCursor;
  const events: ChatEvent[] = [];
  const deadline = Date.now() + 300_000;
  while (Date.now() < deadline) {
    const page = await apiData<Record<string, unknown>>(
      "GET",
      `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    for (const event of nestedArray(page, "items") as ChatEvent[]) {
      if (!events.some((existing) => existing.event_id === event.event_id)) {
        events.push(event);
      }
    }
    cursor = nestedString(page, "latest_cursor") ?? cursor;
    if (
      events.some(
        (event) =>
          event.kind === "assistant_message_completed" ||
          event.kind === "logical_turn_failed" ||
          event.kind === "logical_turn_attention_required",
      )
    ) {
      return { cursor, events };
    }
    await delay(250);
  }
  throw new Error(`timed out waiting for ${sessionId}`);
}

async function latestCursor(sessionId: string): Promise<string> {
  const sessions = await apiData<Record<string, unknown>>(
    "GET",
    "/v1/chat/sessions?limit=500",
  );
  const session = nestedArray(sessions, "items").find(
    (candidate) => nestedString(candidate, "session_id") === sessionId,
  );
  assert.ok(session, `chat inventory must contain ${sessionId}`);
  return nestedString(session, "latest_cursor") ?? `${sessionId}:0`;
}

async function readNativeSnapshot(
  sessionId: string,
): Promise<Record<string, any>> {
  const data = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/context`,
  );
  const snapshot = data.native_snapshot;
  assert.ok(isRecord(snapshot), "context endpoint must return native_snapshot");
  return snapshot as Record<string, any>;
}

async function readAllEvents(sessionId: string): Promise<ChatEvent[]> {
  const data = await apiData<Record<string, unknown>>(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?cursor=${encodeURIComponent(`${sessionId}:0`)}&limit=500`,
  );
  return nestedArray(data, "items") as ChatEvent[];
}

function assertSnapshot(snapshot: Record<string, any>): void {
  assert.equal(snapshot.schemaVersion, 1);
  for (const key of [
    "provider",
    "promptProjection",
    "reservedOutput",
    "admission",
    "providerUsage",
    "durableTranscript",
    "providerState",
    "compaction",
    "diagnostics",
  ]) {
    assert.ok(Object.hasOwn(snapshot, key), `snapshot missing ${key}`);
  }
  assert.equal(
    snapshot.providerUsage.currentRequest.inputTokens.quality,
    "exact",
  );
  assert.equal(snapshot.providerUsage.logicalWake.inputTokens.quality, "exact");
  assert.equal(
    snapshot.promptProjection.protocolProjection.kind,
    "chat_completions",
  );
  assert.equal(snapshot.providerState.stateKind, "chat_completions_messages");
}

function hasCompletedCompaction(
  snapshot: Record<string, any>,
  events: ChatEvent[],
): boolean {
  return (
    hasCompactionArtifact(snapshot) &&
    eventMetadataKinds(events).includes("context_compaction_completed")
  );
}

function hasCompactionArtifact(snapshot: Record<string, any>): boolean {
  return typeof snapshot.compaction?.lastArtifactId === "string";
}

function snapshotsFromEvents(events: ChatEvent[]): Record<string, any>[] {
  return events.flatMap((event) => {
    for (const metadata of eventMetadata(event)) {
      if (
        metadata.kind === "context_accounting_snapshot" &&
        isRecord(metadata.snapshot)
      ) {
        return [metadata.snapshot as Record<string, any>];
      }
    }
    return [];
  });
}

function firstSnapshotAfterMetadata(
  events: ChatEvent[],
  metadataKind: string,
): Record<string, any> | undefined {
  const completionIndex = events.findIndex((event) =>
    eventMetadataKinds(event).includes(metadataKind),
  );
  if (completionIndex < 0) return undefined;
  for (const event of events.slice(completionIndex + 1)) {
    const snapshot = eventMetadata(event).find(
      (metadata) =>
        metadata.kind === "context_accounting_snapshot" &&
        isRecord(metadata.snapshot),
    )?.snapshot;
    if (isRecord(snapshot)) return snapshot as Record<string, any>;
  }
  return undefined;
}

function orderEvents(events: ChatEvent[]): ChatEvent[] {
  return events
    .map((event, index) => ({ event, index }))
    .sort((left, right) => {
      if (
        typeof left.event.sequence_id === "number" &&
        typeof right.event.sequence_id === "number" &&
        left.event.sequence_id !== right.event.sequence_id
      ) {
        return left.event.sequence_id - right.event.sequence_id;
      }
      return left.index - right.index;
    })
    .map(({ event }) => event);
}

function eventMetadataKinds(event: ChatEvent): string[] {
  return eventMetadata(event)
    .map((metadata) => metadata.kind)
    .filter((kind): kind is string => typeof kind === "string");
}

function eventMetadata(event: ChatEvent): Record<string, any>[] {
  const payload = event.payload;
  if (!isRecord(payload)) return [];
  const values = [
    payload.metadata_json,
    payload.metadataJson,
    payload.metadata,
  ];
  return values.flatMap((value) => {
    if (isRecord(value)) return [value as Record<string, any>];
    if (typeof value !== "string") return [];
    try {
      const parsed: unknown = JSON.parse(value);
      return isRecord(parsed) ? [parsed as Record<string, any>] : [];
    } catch {
      return [];
    }
  });
}

function successfulToolCount(events: ChatEvent[]): number {
  return events.filter(
    (event) =>
      event.kind === "tool_call_completed" &&
      nested(event, "payload", "is_error") !== true,
  ).length;
}

function assistantSummary(events: ChatEvent[]): string {
  const completed = events.find(
    (event) => event.kind === "assistant_message_completed",
  );
  return String(nested(completed, "payload", "summary") ?? "");
}

function summarizeSnapshot(
  snapshot: Record<string, any>,
): Record<string, unknown> {
  return {
    schemaVersion: snapshot.schemaVersion,
    sessionId: snapshot.sessionId,
    fillPercent: snapshot.admission?.fillPercent,
    admissionState: snapshot.admission?.state,
    promptInputTokens: snapshot.promptProjection?.inputTokens?.tokens,
    currentRequestInputTokens:
      snapshot.providerUsage?.currentRequest?.inputTokens?.tokens,
    logicalWakeInputTokens:
      snapshot.providerUsage?.logicalWake?.inputTokens?.tokens,
    durableMessageCount: snapshot.durableTranscript?.messageCount,
    providerStateKind: snapshot.providerState?.stateKind,
    compaction: snapshot.compaction,
  };
}

function durableMessageCount(
  snapshot: Record<string, any>,
  label: string,
): number {
  const count = snapshot.durableTranscript?.messageCount;
  assert.ok(
    Number.isSafeInteger(count) && count >= 0,
    `${label} snapshot must report a durable transcript message count`,
  );
  return count as number;
}

function eventReference(event: ChatEvent): Record<string, unknown> {
  return {
    eventId: event.event_id,
    sequenceId: event.sequence_id,
    kind: event.kind,
    wakeId: nestedString(event, "payload", "wake_id"),
    metadataKinds: eventMetadataKinds(event),
  };
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
    responsesDialect: provider.responsesDialect,
    chatCompletionsDialect: provider.chatCompletionsDialect,
    thinkingMode: provider.thinkingMode,
    reasoningHistory: provider.reasoningHistory,
    reasoningBudgetTokens: provider.reasoningBudgetTokens,
    promptCaching: provider.promptCaching,
    metadataJson: provider.metadataJson,
    expectedRevision: provider.revision,
    ...overrides,
  };
}

async function waitForService(): Promise<void> {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    try {
      const response = await api("GET", "/v1/admin/healthz");
      if (
        response.status === 200 &&
        nested(response.json, "data", "ok") === true
      ) {
        return;
      }
    } catch {
      // Expected while systemd is restarting the debug service.
    }
    await delay(250);
  }
  throw new Error(`${serviceUnit} did not become healthy`);
}

async function assertServiceSourceRevision(): Promise<void> {
  const health = await apiData<Record<string, unknown>>(
    "GET",
    "/v1/admin/healthz",
  );
  assert.equal(
    health.sourceRevision,
    expectedSourceRevision,
    `debug service source revision must match the exact certification head ${expectedSourceRevision}`,
  );
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
): Promise<ApiResponse> {
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
    // Keep raw text for the assertion.
  }
  return { status: response.status, text, json };
}

function nested(value: unknown, ...path: string[]): unknown {
  let current = value;
  for (const key of path) {
    if (!isRecord(current)) return undefined;
    current = current[key];
  }
  return current;
}

function nestedString(value: unknown, ...path: string[]): string | undefined {
  const result = nested(value, ...path);
  return typeof result === "string" ? result : undefined;
}

function nestedArray(value: unknown, ...path: string[]): unknown[] {
  const result = nested(value, ...path);
  return Array.isArray(result) ? result : [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
