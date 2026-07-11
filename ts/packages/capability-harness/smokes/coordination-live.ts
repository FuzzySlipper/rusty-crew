import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";

import {
  CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  buildEvidenceComparison,
  validateCapabilityScenario,
  writeCapabilityArtifacts,
  type CapabilityEvidencePacket,
  type RuntimeCoordinationEvidence,
  type RuntimeEvidence,
} from "../src/index.js";

const serviceBaseUrl =
  process.env.RUSTY_CREW_CAPABILITY_SERVICE_URL ?? "http://127.0.0.1:9348";
const runtimeId =
  process.env.RUSTY_CREW_CAPABILITY_CODEX_RUNTIME ?? "rv-live-codex-5516";
const senderBindingId =
  process.env.RUSTY_CREW_CAPABILITY_CODEX_SENDER_BINDING ??
  "rv-codex-5516-a-binding";
const peerBindingId =
  process.env.RUSTY_CREW_CAPABILITY_CODEX_PEER_BINDING ??
  "rv-codex-5516-b-binding";
const directSessionId =
  process.env.RUSTY_CREW_CAPABILITY_DIRECT_SESSION ?? "tester-session";
const directAgentId =
  process.env.RUSTY_CREW_CAPABILITY_DIRECT_AGENT ?? "tester";
const artifactRoot =
  process.env.RUSTY_CREW_CAPABILITY_ARTIFACT_ROOT ??
  `/tmp/rusty-crew-coordination-${Date.now()}`;
const timeoutMs = Number(
  process.env.RUSTY_CREW_CAPABILITY_TURN_TIMEOUT_MS ?? 180_000,
);
const runId = `coordination-${Date.now()}-${randomUUID().slice(0, 8)}`;

interface Envelope<T> {
  ok: boolean;
  data: T;
}

interface ExternalBinding {
  bindingId: string;
  runtimeId: string;
  sessionId: string;
  agentId: string;
  nativeThreadId?: string;
}

interface DeliveryReceipt {
  request: {
    deliveryId: string;
    messageId: string;
    fromAgentId: string;
    toAgentId: string;
  };
  status: string;
  activation?: Record<string, unknown>;
}

interface AgentRound {
  roundId: string;
  correlationId: string;
  senderAgentId: string;
  senderSessionId: string;
  recipientAgentId: string;
  recipientSessionId: string;
  replyMessageId?: string | null;
  status: string;
  terminalReasonCode?: string | null;
}

interface ExternalEvent {
  [key: string]: unknown;
  sequenceId: number;
  kind: string;
  nativeThreadId?: string;
  nativeTurnId?: string;
  itemId?: string;
  payload: Record<string, unknown>;
}

interface ChatEvent {
  [key: string]: unknown;
  kind: string;
  payload: Record<string, unknown>;
}

interface DirectionEvidence {
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  delivery: DeliveryReceipt;
  round: AgentRound;
  native: RuntimeCoordinationEvidence["native"];
  toolEvents: Array<Record<string, unknown>>;
  lifecycleEvents: Array<Record<string, unknown>>;
  interactions: Array<Record<string, unknown>>;
  raw: Record<string, unknown>;
}

const scenario = validateCapabilityScenario({
  id: "cross_runtime_agent_coordination",
  title: "Cross-runtime direct messaging and correlated rounds",
  prompt:
    "Exchange identity-bound messages and correlated replies across direct and Codex runtimes.",
  fixture: { kind: "directory", sourceRef: "service://debug/coordination" },
  requiredCapabilities: [
    "direct_agent_message",
    "correlated_round",
    "dynamic_tool_identity",
    "durable_deduplication",
    "ttl_expiry",
    "restart_reconciliation",
  ],
  permittedEffects: [
    "debug_service_turns",
    "debug_service_restart",
    "durable_coordination_records",
  ],
  expectedArtifacts: [
    "delivery_ids",
    "round_ids",
    "native_thread_turn_tool_ids",
    "restart_expiry_outcome",
  ],
  validationCommands: ["npm run smoke:coordination-live"],
  runtimeApplicability: {
    codex_app_server: { status: "applicable" },
    direct_brain: { status: "applicable" },
  },
});

const bindings = await listBindings();
const senderBinding = requireBinding(bindings, senderBindingId);
const peerBinding = requireBinding(bindings, peerBindingId);
assert.equal(senderBinding.runtimeId, runtimeId);
assert.equal(peerBinding.runtimeId, runtimeId);
assert.equal(typeof senderBinding.nativeThreadId, "string");
assert.equal(typeof peerBinding.nativeThreadId, "string");

console.log("[coordination] direct brain -> Codex");
const directToCodex = await runDirectToCodex(peerBinding);
console.log("[coordination] Codex -> direct brain");
const codexToDirect = await runCodexRound({
  binding: senderBinding,
  recipientAgentId: directAgentId,
  targetInstruction: (correlationId) =>
    `Call send_agent_message exactly once with toAgentId ${senderBinding.agentId}, body CODEX_DIRECT_REPLY_${runId}, and correlationId ${correlationId}. Then finish the turn.`,
  label: "codex-direct",
});
console.log("[coordination] Codex -> Codex");
const codexToCodex = await runCodexRound({
  binding: senderBinding,
  recipientAgentId: peerBinding.agentId,
  targetInstruction: (correlationId) =>
    `Call rusty_crew.send_agent_message exactly once with recipient ${senderBinding.agentId}, body CODEX_CODEX_REPLY_${runId}, and correlationId ${correlationId}. Then finish the turn.`,
  label: "codex-codex",
});
console.log("[coordination] pending round restart and expiry");
const restart = await runRestartExpiry(senderBinding);

const directCoordination: RuntimeCoordinationEvidence = {
  deliveries: [deliveryEvidence(directToCodex.delivery)],
  rounds: [
    roundEvidence(directToCodex.round),
    roundEvidence(codexToDirect.round),
  ],
  native: directToCodex.native,
};
const codexCoordination: RuntimeCoordinationEvidence = {
  deliveries: [
    deliveryEvidence(codexToDirect.delivery),
    deliveryEvidence(codexToCodex.delivery),
    deliveryEvidence(restart.delivery),
  ],
  rounds: [
    roundEvidence(codexToDirect.round),
    roundEvidence(codexToCodex.round),
    roundEvidence(restart.round),
  ],
  native: [...codexToDirect.native, ...codexToCodex.native, restart.native],
  duplicate: {
    exercised: true,
    idempotent: restart.duplicateIdempotent,
    deliveryId: restart.delivery.request.deliveryId,
  },
  expiry: {
    exercised: true,
    status: restart.round.status,
    ...(typeof restart.round.terminalReasonCode !== "string"
      ? {}
      : { reasonCode: restart.round.terminalReasonCode }),
  },
};

const directRuntime = runtimeEvidence({
  runtimeId: "direct-pi-agent",
  runtimeKind: "direct_brain",
  backend: serviceBaseUrl,
  startedAt: directToCodex.startedAt,
  finishedAt: codexToDirect.finishedAt,
  durationMs: directToCodex.durationMs + codexToDirect.durationMs,
  tools: ["agent_round", "send_agent_message"],
  lifecycleEvents: directToCodex.lifecycleEvents,
  toolEvents: directToCodex.toolEvents,
  interactions: [
    ...directToCodex.interactions,
    { direction: "codex_to_direct", roundId: codexToDirect.round.roundId },
  ],
  coordination: directCoordination,
  restart: {
    exercised: true,
    recovered: restart.serviceRecovered,
    evidence: `pending round ${restart.round.roundId} became ${restart.round.status}`,
  },
  effectiveConfig: { directSessionId, directAgentId },
});
const codexRuntime = runtimeEvidence({
  runtimeId: "codex-app-server",
  runtimeKind: "codex_app_server",
  backend: `service:${runtimeId}`,
  startedAt: codexToDirect.startedAt,
  finishedAt: restart.finishedAt,
  durationMs:
    codexToDirect.durationMs + codexToCodex.durationMs + restart.durationMs,
  tools: ["rusty_crew.agent_round", "rusty_crew.send_agent_message"],
  lifecycleEvents: [
    ...codexToDirect.lifecycleEvents,
    ...codexToCodex.lifecycleEvents,
    ...restart.lifecycleEvents,
  ],
  toolEvents: [
    ...codexToDirect.toolEvents,
    ...codexToCodex.toolEvents,
    restart.toolEvent,
  ],
  interactions: [
    { direction: "codex_to_direct", roundId: codexToDirect.round.roundId },
    { direction: "codex_to_codex", roundId: codexToCodex.round.roundId },
  ],
  coordination: codexCoordination,
  restart: {
    exercised: true,
    recovered: restart.serviceRecovered,
    evidence: `controller generation ${restart.controllerGenerationBefore} -> ${restart.controllerGenerationAfter}; round terminal ${restart.round.status}`,
  },
  effectiveConfig: {
    runtimeId,
    senderBindingId,
    peerBindingId,
    exactThreadResume: true,
  },
});
const runtimes = [directRuntime, codexRuntime];
const packet: CapabilityEvidencePacket = {
  schemaVersion: CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  runId,
  createdAt: new Date().toISOString(),
  scenario,
  runtimes,
  comparison: buildEvidenceComparison(runtimes),
};
assert.equal(
  Object.values(packet.comparison.scenarioPassedByRuntime).every(Boolean),
  true,
);
await writeCapabilityArtifacts(artifactRoot, packet, {
  directToCodex: directToCodex.raw,
  codexToDirect: codexToDirect.raw,
  codexToCodex: codexToCodex.raw,
  restart,
});

console.log(
  JSON.stringify(
    {
      runId,
      artifactRoot,
      directToCodexRoundId: directToCodex.round.roundId,
      codexToDirectRoundId: codexToDirect.round.roundId,
      codexToCodexRoundId: codexToCodex.round.roundId,
      restartRoundId: restart.round.roundId,
      restartRoundStatus: restart.round.status,
    },
    null,
    2,
  ),
);

async function runDirectToCodex(
  binding: ExternalBinding,
): Promise<DirectionEvidence> {
  const started = Date.now();
  const startedAt = new Date(started).toISOString();
  const chatCursor = String(
    (
      await getJson<{ session: { latest_cursor: string } }>(
        `/v1/chat/sessions/${encodeURIComponent(directSessionId)}?limit=1`,
      )
    ).session.latest_cursor,
  );
  const externalCursor = await latestExternalSequence();
  const correlationId = `${runId}-direct-codex`;
  const clientMessageId = `${correlationId}-trigger`;
  const targetInstruction = `Call rusty_crew.send_agent_message exactly once with recipient ${directAgentId}, body DIRECT_CODEX_REPLY_${runId}, and correlationId ${correlationId}. Then finish the turn.`;
  const response = await fetch(
    `${serviceBaseUrl}/v1/chat/sessions/${encodeURIComponent(directSessionId)}/messages`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": clientMessageId,
      },
      body: JSON.stringify({
        actor: { id: "capability-harness", kind: "human" },
        body: agentRoundPrompt(
          binding.agentId,
          correlationId,
          targetInstruction,
        ),
        client_message_id: clientMessageId,
        reason: "cross-runtime coordination certification",
      }),
      signal: AbortSignal.timeout(timeoutMs),
    },
  );
  const trigger = (await response.json()) as unknown;
  assert.equal(response.status, 202, JSON.stringify(trigger));
  const chatEvents = await readChatUntilTerminal(chatCursor);
  const startedTool = chatEvents.find(
    (event) =>
      event.kind === "tool_call_started" &&
      event.payload.tool_name === "agent_round",
  );
  assert(startedTool !== undefined, "direct agent_round was not observed");
  const debugDetailId = requiredString(startedTool.payload.debug_detail_id);
  const toolDetail = await getJson<{
    tool_call_id: string;
    wake_id: string;
    status: string;
  }>(
    `/v1/chat/sessions/${encodeURIComponent(directSessionId)}/tool-calls/${encodeURIComponent(debugDetailId)}`,
  );
  assert.equal(toolDetail.status, "completed");
  const roundId = [
    "round",
    directSessionId,
    toolDetail.wake_id,
    toolDetail.tool_call_id,
  ].join(":");
  const round = await getJson<AgentRound>(
    `/v1/agent-rounds/${encodeURIComponent(roundId)}`,
  );
  assertRepliedRound(round, directAgentId, binding.agentId, correlationId);
  const delivery = await getJson<DeliveryReceipt>(
    `/v1/agent-deliveries/${encodeURIComponent(`round-delivery:${roundId}`)}`,
  );
  const externalEvents = await externalEventsAfter(externalCursor);
  const replyToolId = round.replyMessageId?.split(":").at(-1);
  const replyTool = externalEvents.find(
    (event) =>
      event.nativeThreadId === binding.nativeThreadId &&
      event.itemId === replyToolId &&
      event.kind === "dynamic_tool_activity" &&
      event.payload.nativeMethod === "item/completed" &&
      event.payload.success === true,
  );
  assert(
    replyTool !== undefined,
    "Codex correlated reply tool was not observed",
  );
  const finished = Date.now();
  return {
    startedAt,
    finishedAt: new Date(finished).toISOString(),
    durationMs: finished - started,
    delivery,
    round,
    native: [nativeEvidence(binding, replyTool)],
    toolEvents: chatEvents.filter((event) =>
      event.kind.startsWith("tool_call_"),
    ),
    lifecycleEvents: chatEvents.filter((event) =>
      event.kind.startsWith("assistant_turn_"),
    ),
    interactions: [{ direction: "direct_to_codex", roundId }],
    raw: { trigger, chatEvents, toolDetail, round, delivery, externalEvents },
  };
}

async function runCodexRound(input: {
  binding: ExternalBinding;
  recipientAgentId: string;
  targetInstruction(correlationId: string): string;
  label: string;
}): Promise<DirectionEvidence> {
  const started = Date.now();
  const startedAt = new Date(started).toISOString();
  const cursor = await latestExternalSequence();
  const correlationId = `${runId}-${input.label}`;
  const delivery = await postBindingMessage(
    input.binding.bindingId,
    `${correlationId}-trigger`,
    agentRoundPrompt(
      input.recipientAgentId,
      correlationId,
      input.targetInstruction(correlationId),
      true,
    ),
    60_000,
  );
  assert.equal(
    ["external_turn_requested", "queued_for_next_turn"].includes(
      String(delivery.activation?.type),
    ),
    true,
  );
  const tool = await waitForExternalTool({
    afterSequence: cursor,
    threadId: requiredString(input.binding.nativeThreadId),
    tool: "agent_round",
    terminal: true,
  });
  assert.equal(tool.completed?.payload.success, true);
  const roundId = [
    "codex-round",
    input.binding.bindingId,
    input.binding.nativeThreadId,
    tool.started.nativeTurnId,
    tool.started.itemId,
  ].join(":");
  const round = await getJson<AgentRound>(
    `/v1/agent-rounds/${encodeURIComponent(roundId)}`,
  );
  assertRepliedRound(
    round,
    input.binding.agentId,
    input.recipientAgentId,
    correlationId,
  );
  await waitForExternalTurnTerminal(
    cursor,
    requiredString(tool.started.nativeTurnId),
  );
  await delay(250);
  const finished = Date.now();
  const events = await externalEventsAfter(cursor);
  return {
    startedAt,
    finishedAt: new Date(finished).toISOString(),
    durationMs: finished - started,
    delivery,
    round,
    native: [nativeEvidence(input.binding, tool.started)],
    toolEvents: [tool.started, tool.completed as ExternalEvent],
    lifecycleEvents: events.filter(
      (event) =>
        event.nativeTurnId === tool.started.nativeTurnId &&
        event.kind === "turn_lifecycle",
    ),
    interactions: [{ direction: input.label, roundId }],
    raw: { events, round, delivery },
  };
}

async function waitForExternalTurnTerminal(
  afterSequence: number,
  turnId: string,
): Promise<ExternalEvent> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const terminal = (await externalEventsAfter(afterSequence)).find(
      (event) =>
        event.kind === "turn_lifecycle" &&
        event.nativeTurnId === turnId &&
        (event.payload.nativeMethod === "turn/completed" ||
          event.payload.nativeMethod === "turn/interrupted"),
    );
    if (terminal !== undefined) return terminal;
    await delay(100);
  }
  throw new Error(`external turn ${turnId} did not reach a terminal event`);
}

async function runRestartExpiry(binding: ExternalBinding): Promise<{
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  delivery: DeliveryReceipt;
  round: AgentRound;
  native: RuntimeCoordinationEvidence["native"][number];
  toolEvent: ExternalEvent;
  lifecycleEvents: ExternalEvent[];
  duplicateIdempotent: boolean;
  serviceRecovered: boolean;
  controllerGenerationBefore: number;
  controllerGenerationAfter: number;
}> {
  const started = Date.now();
  const startedAt = new Date(started).toISOString();
  const before = await runtimeControllerStatus();
  const cursor = await latestExternalSequence();
  const correlationId = `${runId}-restart-expiry`;
  const label = `${correlationId}-trigger`;
  const body = agentRoundPrompt(
    directAgentId,
    correlationId,
    `Call terminal exactly once to run sleep 30. After it finishes, call send_agent_message with toAgentId ${binding.agentId}, body LATE_REPLY_${runId}, and correlationId ${correlationId}.`,
    true,
    12_000,
  );
  const delivery = await postBindingMessage(
    binding.bindingId,
    label,
    body,
    60_000,
  );
  const duplicate = await postBindingMessage(
    binding.bindingId,
    label,
    body,
    60_000,
  );
  const duplicateIdempotent =
    JSON.stringify(duplicate) === JSON.stringify(delivery);
  assert.equal(duplicateIdempotent, true);
  const tool = await waitForExternalTool({
    afterSequence: cursor,
    threadId: requiredString(binding.nativeThreadId),
    tool: "agent_round",
    terminal: false,
  });
  const roundId = [
    "codex-round",
    binding.bindingId,
    binding.nativeThreadId,
    tool.started.nativeTurnId,
    tool.started.itemId,
  ].join(":");
  const pending = await waitForRound(
    roundId,
    (round) => round.status === "pending",
  );
  assert.equal(pending.correlationId, correlationId);

  execFileSync("systemctl", ["--user", "restart", "rusty-crew-debug.service"]);
  await waitForServiceReady();
  const after = await runtimeControllerStatus();
  assert.notEqual(after.controllerGeneration, before.controllerGeneration);
  const round = await waitForRound(
    roundId,
    (candidate) => candidate.status !== "pending",
  );
  assert.equal(round.status, "expired");
  assert.equal(round.terminalReasonCode, "agent_round_timeout");
  assert.equal(round.replyMessageId == null, true);
  const replayedDelivery = await getJson<DeliveryReceipt>(
    `/v1/agent-deliveries/${encodeURIComponent(delivery.request.deliveryId)}`,
  );
  assert.deepEqual(replayedDelivery, delivery);
  const finished = Date.now();
  return {
    startedAt,
    finishedAt: new Date(finished).toISOString(),
    durationMs: finished - started,
    delivery,
    round,
    native: nativeEvidence(binding, tool.started),
    toolEvent: tool.started,
    lifecycleEvents: (await externalEventsAfter(cursor)).filter(
      (event) => event.nativeTurnId === tool.started.nativeTurnId,
    ),
    duplicateIdempotent,
    serviceRecovered: true,
    controllerGenerationBefore: before.controllerGeneration,
    controllerGenerationAfter: after.controllerGeneration,
  };
}

function runtimeEvidence(input: {
  runtimeId: string;
  runtimeKind: RuntimeEvidence["runtimeKind"];
  backend: string;
  startedAt: string;
  finishedAt: string;
  durationMs: number;
  tools: string[];
  lifecycleEvents: Array<Record<string, unknown>>;
  toolEvents: Array<Record<string, unknown>>;
  interactions: Array<Record<string, unknown>>;
  coordination: RuntimeCoordinationEvidence;
  restart: RuntimeEvidence["restart"];
  effectiveConfig: Record<string, unknown>;
}): RuntimeEvidence {
  return {
    ...input,
    model:
      input.runtimeKind === "direct_brain"
        ? "tester-chat"
        : "codex-account-default",
    effort: "medium",
    commands: [],
    fileChanges: [],
    tests: [{ command: "durable coordination assertions", passed: true }],
    capabilities: scenario.requiredCapabilities.map((capability) => ({
      capability,
      support: "supported",
      evidence: "durable live-service identities and terminal state observed",
    })),
    failures: [],
  };
}

function agentRoundPrompt(
  recipientAgentId: string,
  correlationId: string,
  targetInstruction: string,
  codex = false,
  roundTimeoutMs = 90_000,
): string {
  const tool = codex ? "rusty_crew.agent_round" : "agent_round";
  const recipientField = codex ? "recipient" : "toAgentId";
  return [
    `Call ${tool} exactly once.`,
    `Use ${recipientField} ${recipientAgentId}.`,
    `Use correlationId ${correlationId}.`,
    `Use timeoutMs ${roundTimeoutMs}.`,
    `Use this body exactly: ${targetInstruction}`,
    "After the tool returns, finish the turn.",
  ].join("\n");
}

async function readChatUntilTerminal(
  initialCursor: string,
): Promise<ChatEvent[]> {
  const events: ChatEvent[] = [];
  let cursor = initialCursor;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const page = await getJson<{
      items: ChatEvent[];
      latest_cursor: string;
      has_more: boolean;
    }>(
      `/v1/chat/sessions/${encodeURIComponent(directSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    events.push(...page.items);
    cursor = String(page.latest_cursor ?? cursor);
    if (
      events.some((event) => event.kind === "assistant_turn_finished") &&
      !page.has_more
    ) {
      return events;
    }
    await delay(100);
  }
  throw new Error(`direct coordination turn exceeded ${timeoutMs}ms`);
}

async function waitForExternalTool(input: {
  afterSequence: number;
  threadId: string;
  tool: string;
  terminal: boolean;
}): Promise<{ started: ExternalEvent; completed?: ExternalEvent }> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const events = await externalEventsAfter(input.afterSequence);
    const started = events.find(
      (event) =>
        event.kind === "dynamic_tool_activity" &&
        event.nativeThreadId === input.threadId &&
        event.payload.tool === input.tool &&
        event.payload.nativeMethod === "item/started",
    );
    const completed = events.find(
      (event) =>
        event.kind === "dynamic_tool_activity" &&
        event.itemId === started?.itemId &&
        event.payload.nativeMethod === "item/completed",
    );
    if (started !== undefined && (!input.terminal || completed !== undefined)) {
      return { started, ...(completed === undefined ? {} : { completed }) };
    }
    await delay(100);
  }
  throw new Error(`external ${input.tool} activity exceeded ${timeoutMs}ms`);
}

async function waitForRound(
  roundId: string,
  predicate: (round: AgentRound) => boolean,
): Promise<AgentRound> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const response = await fetch(
      `${serviceBaseUrl}/v1/agent-rounds/${encodeURIComponent(roundId)}`,
    );
    if (response.status === 200) {
      const envelope = (await response.json()) as Envelope<AgentRound>;
      if (predicate(envelope.data)) return envelope.data;
    }
    await delay(100);
  }
  throw new Error(`agent round ${roundId} did not reach expected state`);
}

async function postBindingMessage(
  bindingId: string,
  label: string,
  body: string,
  ttlMs: number,
): Promise<DeliveryReceipt> {
  const response = await fetch(
    `${serviceBaseUrl}/v1/external-bindings/${encodeURIComponent(bindingId)}/messages`,
    {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        deliveryId: `${label}-delivery`,
        idempotencyKey: `${label}-delivery`,
        messageId: `${label}-message`,
        body,
        ttlMs,
      }),
      signal: AbortSignal.timeout(30_000),
    },
  );
  const envelope = (await response.json()) as Envelope<DeliveryReceipt>;
  assert.equal(response.status, 200, JSON.stringify(envelope));
  assert.equal(envelope.ok, true);
  return envelope.data;
}

async function listBindings(): Promise<ExternalBinding[]> {
  return (
    await getJson<{ bindings: ExternalBinding[] }>("/v1/external-bindings")
  ).bindings;
}

function requireBinding(
  bindings: readonly ExternalBinding[],
  bindingId: string,
): ExternalBinding {
  const binding = bindings.find(
    (candidate) => candidate.bindingId === bindingId,
  );
  assert(binding !== undefined, `external binding ${bindingId} was not found`);
  return binding;
}

async function latestExternalSequence(): Promise<number> {
  let cursor = 0;
  while (true) {
    const page = await externalEventsAfter(cursor);
    if (page.length === 0) return cursor;
    cursor = page.at(-1)?.sequenceId ?? cursor;
    if (page.length < 1_000) return cursor;
  }
}

async function externalEventsAfter(
  afterSequence: number,
): Promise<ExternalEvent[]> {
  return (
    await getJson<{ events: ExternalEvent[] }>(
      `/v1/external-runtimes/${encodeURIComponent(runtimeId)}/events?after=${afterSequence}&limit=1000`,
    )
  ).events;
}

async function runtimeControllerStatus(): Promise<{
  controllerGeneration: number;
  driverState: string;
}> {
  const status = await getJson<{
    registration: { observedState: string };
    controller: { controllerGeneration: number; driverState: string };
  }>(`/v1/external-runtimes/${encodeURIComponent(runtimeId)}`);
  assert.equal(status.registration.observedState, "ready");
  assert.equal(status.controller.driverState, "ready");
  return status.controller;
}

async function waitForServiceReady(): Promise<void> {
  const deadline = Date.now() + 60_000;
  while (Date.now() < deadline) {
    try {
      const diagnostics = await getJson<{
        health: { readiness: { ready: boolean } };
      }>("/v1/admin/diagnostics");
      if (diagnostics.health.readiness.ready) {
        await runtimeControllerStatus();
        return;
      }
    } catch {
      // Process replacement briefly refuses connections.
    }
    await delay(250);
  }
  throw new Error("debug service did not recover after restart");
}

async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${serviceBaseUrl}${path}`, {
    signal: AbortSignal.timeout(30_000),
  });
  const envelope = (await response.json()) as Envelope<T>;
  assert.equal(response.status, 200, JSON.stringify(envelope));
  assert.equal(envelope.ok, true, JSON.stringify(envelope));
  return envelope.data;
}

function assertRepliedRound(
  round: AgentRound,
  senderAgentId: string,
  recipientAgentId: string,
  correlationId: string,
): void {
  assert.equal(round.status, "replied");
  assert.equal(round.senderAgentId, senderAgentId);
  assert.equal(round.recipientAgentId, recipientAgentId);
  assert.equal(round.correlationId, correlationId);
  assert.equal(typeof round.replyMessageId, "string");
}

function deliveryEvidence(
  receipt: DeliveryReceipt,
): RuntimeCoordinationEvidence["deliveries"][number] {
  return {
    deliveryId: receipt.request.deliveryId,
    messageId: receipt.request.messageId,
    fromAgentId: receipt.request.fromAgentId,
    toAgentId: receipt.request.toAgentId,
    status: receipt.status,
    ...(receipt.activation === undefined
      ? {}
      : { activation: receipt.activation }),
  };
}

function roundEvidence(
  round: AgentRound,
): RuntimeCoordinationEvidence["rounds"][number] {
  return {
    roundId: round.roundId,
    correlationId: round.correlationId,
    senderAgentId: round.senderAgentId,
    senderSessionId: round.senderSessionId,
    recipientAgentId: round.recipientAgentId,
    recipientSessionId: round.recipientSessionId,
    status: round.status,
    ...(typeof round.terminalReasonCode !== "string"
      ? {}
      : { terminalReasonCode: round.terminalReasonCode }),
  };
}

function nativeEvidence(
  binding: ExternalBinding,
  event: ExternalEvent,
): RuntimeCoordinationEvidence["native"][number] {
  return {
    runtimeId,
    bindingId: binding.bindingId,
    threadId: requiredString(event.nativeThreadId),
    turnId: requiredString(event.nativeTurnId),
    toolCallId: requiredString(event.itemId),
  };
}

function requiredString(value: unknown): string {
  assert.equal(typeof value, "string");
  assert.notEqual(value, "");
  return value as string;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
