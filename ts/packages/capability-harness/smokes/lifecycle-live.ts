import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";

import {
  CODEX_APP_SERVER_PROTOCOL,
  CodexAppServerDriver,
  UnixWebSocketTransport,
  type NeutralExternalRuntimeEvent,
} from "@rusty-crew/external-runtime-codex";

import {
  CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  RecordingCodexAuthority,
  buildEvidenceComparison,
  validateCapabilityScenario,
  writeCapabilityArtifacts,
  type CapabilityEvidencePacket,
  type CapabilityScenario,
  type RuntimeEvidence,
} from "../src/index.js";

const socketPath =
  process.env.CODEX_APP_SERVER_SOCKET ??
  "/run/user/1001/codex-app-server/app-server.sock";
const serviceBaseUrl =
  process.env.RUSTY_CREW_CAPABILITY_SERVICE_URL ?? "http://127.0.0.1:9348";
const responsesSessionId =
  process.env.RUSTY_CREW_CAPABILITY_RESPONSES_SESSION ??
  "responses-cert-5389-session";
const artifactRoot =
  process.env.RUSTY_CREW_CAPABILITY_ARTIFACT_ROOT ??
  `/tmp/rusty-crew-lifecycle-${Date.now()}`;
const timeoutMs = Number(
  process.env.RUSTY_CREW_CAPABILITY_TURN_TIMEOUT_MS ?? 300_000,
);
const runId = `lifecycle-${Date.now()}-${randomUUID().slice(0, 8)}`;

const applicableCodexOnly = {
  codex_app_server: { status: "applicable" } as const,
  direct_brain: {
    status: "unsupported" as const,
    reason: "the direct chat API has no equivalent interactive control",
  },
};

const scenarios = {
  interaction: scenario({
    id: "structured_user_input",
    title: "Structured user input resolution",
    prompt:
      "Use request_user_input to ask which certification color to use, offering blue and green. After the answer, reply CAPABILITY_INPUT_OK:<answer>.",
    requiredCapabilities: ["structured_user_input"],
    expectedArtifacts: ["server_request", "assistant_response"],
    runtimeApplicability: applicableCodexOnly,
  }),
  elicitation: scenario({
    id: "approval_mcp_elicitation",
    title: "Approval and MCP elicitation availability",
    prompt: "Inspect advertised approval and MCP elicitation capability.",
    requiredCapabilities: ["approval_or_mcp_elicitation"],
    expectedArtifacts: ["unsupported_capability"],
    runtimeApplicability: {
      codex_app_server: {
        status: "unsupported",
        reason:
          "approval policy is never and no configured MCP server advertises elicitation",
      },
      direct_brain: {
        status: "unsupported",
        reason:
          "direct chat has no approval or MCP elicitation response surface",
      },
    },
  }),
  compaction: scenario({
    id: "compaction_continuation",
    title: "Compaction and second-turn continuation",
    prompt: "Preserve a marker across explicit thread compaction.",
    requiredCapabilities: ["compaction", "second_turn_continuation"],
    expectedArtifacts: ["compaction_event", "assistant_response"],
    runtimeApplicability: applicableCodexOnly,
  }),
  control: scenario({
    id: "interrupt_and_steer",
    title: "Active turn steer and interrupt",
    prompt: "Exercise explicit active-turn controls with native turn identity.",
    requiredCapabilities: ["turn_steer", "turn_interrupt"],
    expectedArtifacts: ["control_receipt", "terminal_turn_event"],
    runtimeApplicability: applicableCodexOnly,
  }),
  appRestart: scenario({
    id: "app_server_exact_thread_restart",
    title: "App-server process replacement and exact-thread resume",
    prompt: "Resume the exact persisted thread after process replacement.",
    requiredCapabilities: ["process_restart", "exact_thread_resume"],
    expectedArtifacts: ["thread_id", "resume_response", "assistant_response"],
    runtimeApplicability: applicableCodexOnly,
  }),
  crewRestart: scenario({
    id: "crew_service_restart_recovery",
    title: "Rusty Crew service restart and session recovery",
    prompt: "Continue the exact direct-brain session after service restart.",
    requiredCapabilities: ["service_restart", "session_recovery"],
    expectedArtifacts: ["session_id", "assistant_response"],
    runtimeApplicability: {
      codex_app_server: {
        status: "unsupported",
        reason: "scenario applies to the direct Rusty Crew service runtime",
      },
      direct_brain: { status: "applicable" },
    },
  }),
} as const;

let authority = createInteractiveAuthority();
let driver = new CodexAppServerDriver(
  new UnixWebSocketTransport(socketPath),
  authority,
);
await driver.connect();
const started = await driver.threadStart({
  cwd: "/home/.tmp",
  approvalPolicy: "never",
  sandbox: "danger-full-access",
  ephemeral: false,
});
const threadId = started.thread.id;

try {
  console.log(`[lifecycle] structured input on ${threadId}`);
  await writePacket(
    scenarios.interaction,
    [await runStructuredInput(threadId)],
    { threadId },
  );
  await writePacket(
    scenarios.elicitation,
    [
      unsupportedEvidence(
        scenarios.elicitation,
        "codex_app_server",
        "codex-app-server",
        unsupportedReason(
          scenarios.elicitation.runtimeApplicability.codex_app_server,
        ),
      ),
      unsupportedEvidence(
        scenarios.elicitation,
        "direct_brain",
        "direct-responses",
        unsupportedReason(
          scenarios.elicitation.runtimeApplicability.direct_brain,
        ),
      ),
    ],
    { advertised: false },
  );
  console.log("[lifecycle] compaction and continuation");
  await writePacket(scenarios.compaction, [await runCompaction(threadId)], {
    threadId,
  });
  console.log("[lifecycle] steer and interrupt");
  await writePacket(scenarios.control, [await runControls(threadId)], {
    threadId,
  });

  console.log("[lifecycle] app-server process replacement");
  const restartResult = await runAppServerRestart(threadId);
  driver = restartResult.driver;
  authority = restartResult.authority;
  await writePacket(scenarios.appRestart, [restartResult.evidence], {
    threadId,
    resumedThreadId: restartResult.resumedThreadId,
  });

  console.log("[lifecycle] Rusty Crew debug service restart");
  await writePacket(scenarios.crewRestart, [await runCrewRestart()], {
    sessionId: responsesSessionId,
  });

  console.log(
    JSON.stringify(
      {
        runId,
        artifactRoot,
        threadId,
        responsesSessionId,
        scenarios: Object.values(scenarios).map((item) => item.id),
      },
      null,
      2,
    ),
  );
} finally {
  await driver.close().catch(() => undefined);
}

function scenario(
  input: Pick<
    CapabilityScenario,
    | "id"
    | "title"
    | "prompt"
    | "requiredCapabilities"
    | "expectedArtifacts"
    | "runtimeApplicability"
  >,
): CapabilityScenario {
  return validateCapabilityScenario({
    ...input,
    fixture: { kind: "directory", sourceRef: "fixture://lifecycle" },
    permittedEffects: ["runtime_control", "task_local_observation"],
    validationCommands: ["native lifecycle evidence assertion"],
  });
}

function createInteractiveAuthority(): RecordingCodexAuthority {
  return new RecordingCodexAuthority((context) => {
    if (context.request.method === "item/tool/requestUserInput") {
      const answers = Object.fromEntries(
        context.request.params.questions.map((question) => [
          question.id,
          { answers: ["blue"] },
        ]),
      );
      return { type: "success", result: { answers } };
    }
    return {
      type: "error",
      code: -32000,
      message: `lifecycle harness does not permit ${context.request.method}`,
    };
  });
}

async function runStructuredInput(threadId: string): Promise<RuntimeEvidence> {
  const eventStart = authority.events.length;
  const interactionStart = authority.interactions.length;
  const startedAt = new Date();
  const turn = await driver.turnStart({
    threadId,
    input: [textInput(scenarios.interaction.prompt)],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    collaborationMode: {
      mode: "plan",
      settings: {
        model: started.model,
        reasoning_effort: "medium",
        developer_instructions: null,
      },
    },
  });
  await waitForTurnTerminal(authority, turn.turn.id);
  const events = authority.events.slice(eventStart);
  const interactions = authority.interactions.slice(interactionStart);
  const finalResponse = assistantText(events);
  const passed =
    interactions.some(
      (item) =>
        item.method === "item/tool/requestUserInput" &&
        item.resolutionType === "success",
    ) && finalResponse.includes("CAPABILITY_INPUT_OK:blue");
  assert.equal(
    passed,
    true,
    `structured user input must resolve live: ${JSON.stringify({ interactions, finalResponse })}`,
  );
  return codexEvidence(scenarios.interaction, startedAt, events, {
    interactions,
    finalResponse,
    supported: true,
  });
}

async function runCompaction(threadId: string): Promise<RuntimeEvidence> {
  const marker = `compact-${randomUUID().slice(0, 8)}`;
  await completeTextTurn(
    threadId,
    `Remember the exact marker ${marker} and reply CAPABILITY_COMPACT_STORED.`,
  );
  const eventStart = authority.events.length;
  const sequenceBeforeCompaction =
    authority.events.at(-1)?.transportSequence ?? 0;
  const startedAt = new Date();
  await driver.compactThread({ threadId });
  const compactionTurn = await waitForEvent(
    authority,
    (event) =>
      event.transportSequence > sequenceBeforeCompaction &&
      event.method === "turn/started" &&
      event.threadId === threadId,
  );
  assert.ok(compactionTurn.turnId, "compaction must start a native turn");
  await waitForTurnTerminal(authority, compactionTurn.turnId);
  const turn = await startTextTurn(
    threadId,
    "After compaction, reply with CAPABILITY_COMPACT_OK:<the exact marker I asked you to remember>.",
  );
  await waitForTurnTerminal(authority, turn.turn.id);
  const events = authority.events.slice(eventStart);
  const finalResponse = assistantText(events);
  const passed =
    events.some(
      (event) =>
        event.method === "turn/completed" &&
        event.turnId === compactionTurn.turnId,
    ) && finalResponse.includes(`CAPABILITY_COMPACT_OK:${marker}`);
  assert.equal(passed, true, "compaction must preserve second-turn context");
  return codexEvidence(scenarios.compaction, startedAt, events, {
    finalResponse,
    supported: true,
    interactions: [
      { type: "thread_compact_start", turnId: compactionTurn.turnId },
    ],
  });
}

async function runControls(threadId: string): Promise<RuntimeEvidence> {
  const eventStart = authority.events.length;
  const startedAt = new Date();
  const steerTurn = await startTextTurn(
    threadId,
    "Run the shell command sleep 10, then reply ORIGINAL_WAIT_DONE.",
  );
  await waitForEvent(
    authority,
    (event) =>
      event.turnId === steerTurn.turn.id && event.kind === "command_activity",
  );
  await driver.turnSteer({
    threadId,
    expectedTurnId: steerTurn.turn.id,
    input: [textInput("Stop the wait and reply CAPABILITY_STEER_OK.")],
  });
  await waitForTurnTerminal(authority, steerTurn.turn.id);

  const interruptTurn = await startTextTurn(
    threadId,
    "Run the shell command sleep 30, then reply SHOULD_NOT_COMPLETE.",
  );
  await waitForEvent(
    authority,
    (event) =>
      event.turnId === interruptTurn.turn.id &&
      event.kind === "command_activity",
  );
  await driver.turnInterrupt({ threadId, turnId: interruptTurn.turn.id });
  await waitForTurnTerminal(authority, interruptTurn.turn.id);
  const events = authority.events.slice(eventStart);
  const steerText = assistantText(
    events.filter((event) => event.turnId === steerTurn.turn.id),
  );
  const interrupted = events.some(
    (event) =>
      event.turnId === interruptTurn.turn.id &&
      (event.method === "turn/completed" ||
        event.method === "turn/interrupted"),
  );
  assert.equal(steerText.includes("CAPABILITY_STEER_OK"), true);
  assert.equal(interrupted, true);
  return codexEvidence(scenarios.control, startedAt, events, {
    finalResponse: steerText,
    supported: true,
    interactions: [
      { type: "turn_steer", expectedTurnId: steerTurn.turn.id },
      { type: "turn_interrupt", turnId: interruptTurn.turn.id },
    ],
  });
}

async function runAppServerRestart(threadId: string): Promise<{
  driver: CodexAppServerDriver;
  authority: RecordingCodexAuthority;
  evidence: RuntimeEvidence;
  resumedThreadId: string;
}> {
  const marker = `restart-${randomUUID().slice(0, 8)}`;
  await completeTextTurn(
    threadId,
    `Remember exact marker ${marker} and reply CAPABILITY_RESTART_STORED.`,
  );
  const startedAt = new Date();
  await driver.close();
  execFileSync("systemctl", ["--user", "restart", "codex-app-server.service"]);
  await waitFor(() => existsSync(socketPath), "Codex app-server socket");
  const replacementAuthority = createInteractiveAuthority();
  const replacementDriver = new CodexAppServerDriver(
    new UnixWebSocketTransport(socketPath),
    replacementAuthority,
  );
  await retry(() => replacementDriver.connect());
  const resumed = await replacementDriver.threadResume({
    threadId,
    cwd: "/home/.tmp",
    approvalPolicy: "never",
    sandbox: "danger-full-access",
  });
  const turn = await replacementDriver.turnStart({
    threadId,
    input: [
      textInput(
        "After process replacement, reply CAPABILITY_APP_RESTART_OK:<the exact marker I asked you to remember>.",
      ),
    ],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
  });
  await waitForTurnTerminal(replacementAuthority, turn.turn.id);
  const finalResponse = assistantText(replacementAuthority.events);
  assert.equal(
    finalResponse.includes(`CAPABILITY_APP_RESTART_OK:${marker}`),
    true,
  );
  return {
    driver: replacementDriver,
    authority: replacementAuthority,
    resumedThreadId: resumed.thread.id,
    evidence: codexEvidence(
      scenarios.appRestart,
      startedAt,
      replacementAuthority.events,
      {
        finalResponse,
        supported: resumed.thread.id === threadId,
        restart: {
          exercised: true,
          recovered: resumed.thread.id === threadId,
          evidence: `resumed exact thread ${resumed.thread.id}`,
        },
      },
    ),
  };
}

async function runCrewRestart(): Promise<RuntimeEvidence> {
  const marker = `crew-restart-${randomUUID().slice(0, 8)}`;
  await submitDirectTurn(
    `Remember exact marker ${marker} and reply CAPABILITY_CREW_RESTART_STORED.`,
  );
  const startedAt = new Date();
  execFileSync("systemctl", ["--user", "restart", "rusty-crew-debug.service"]);
  await waitForServiceReady();
  const result = await submitDirectTurn(
    "After service restart, reply CAPABILITY_CREW_RESTART_OK:<the exact marker I asked you to remember>.",
  );
  const recovered = result.finalResponse.includes(
    `CAPABILITY_CREW_RESTART_OK:${marker}`,
  );
  assert.equal(
    recovered,
    true,
    "direct session must recover after service restart",
  );
  const finishedAt = new Date();
  return {
    runtimeId: "direct-responses",
    runtimeKind: "direct_brain",
    backend: serviceBaseUrl,
    model: "gpt",
    effort: "medium",
    effectiveConfig: { sessionId: responsesSessionId },
    tools: result.tools,
    startedAt: startedAt.toISOString(),
    finishedAt: finishedAt.toISOString(),
    durationMs: finishedAt.getTime() - startedAt.getTime(),
    lifecycleEvents: result.events.filter((event) =>
      /assistant_turn/.test(event.kind),
    ),
    toolEvents: result.events.filter((event) => /tool_call/.test(event.kind)),
    commands: [],
    fileChanges: [],
    tests: [{ command: "exact session readback", passed: recovered }],
    interactions: [{ type: "service_restart", service: "rusty-crew-debug" }],
    capabilities: scenarios.crewRestart.requiredCapabilities.map(
      (capability) => ({
        capability,
        support: recovered ? "supported" : "unsupported",
        evidence: recovered
          ? `session ${responsesSessionId} recovered`
          : undefined,
        reason: recovered ? undefined : "exact marker was not recalled",
      }),
    ),
    finalResponse: result.finalResponse,
    failures: recovered
      ? []
      : [{ code: "restart_recovery_failed", message: result.finalResponse }],
    restart: {
      exercised: true,
      recovered,
      evidence: `service restart retained session ${responsesSessionId}`,
    },
  };
}

async function writePacket(
  scenario: CapabilityScenario,
  runtimes: RuntimeEvidence[],
  raw: Record<string, unknown>,
): Promise<void> {
  const packet: CapabilityEvidencePacket = {
    schemaVersion: CAPABILITY_EVIDENCE_SCHEMA_VERSION,
    runId: `${runId}-${scenario.id}`,
    createdAt: new Date().toISOString(),
    scenario,
    runtimes,
    comparison: buildEvidenceComparison(runtimes),
  };
  await writeCapabilityArtifacts(`${artifactRoot}/${scenario.id}`, packet, raw);
}

function codexEvidence(
  scenario: CapabilityScenario,
  startedAt: Date,
  events: NeutralExternalRuntimeEvent[],
  input: {
    finalResponse: string;
    supported: boolean;
    interactions?: Array<Record<string, unknown>>;
    restart?: RuntimeEvidence["restart"];
  },
): RuntimeEvidence {
  const finishedAt = new Date();
  return {
    runtimeId: "codex-app-server",
    runtimeKind: "codex_app_server",
    backend: `unix-websocket:${socketPath}`,
    executable: {
      version: CODEX_APP_SERVER_PROTOCOL.cliVersion,
      sha256: CODEX_APP_SERVER_PROTOCOL.nativeExecutableSha256,
      protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
    },
    model: "codex-account-default",
    effort: "medium",
    effectiveConfig: { threadId: events.at(-1)?.threadId },
    tools: [
      ...new Set(
        events
          .filter((event) => /activity/.test(event.kind))
          .map((event) => event.kind),
      ),
    ],
    startedAt: startedAt.toISOString(),
    finishedAt: finishedAt.toISOString(),
    durationMs: finishedAt.getTime() - startedAt.getTime(),
    lifecycleEvents: events
      .filter((event) => /lifecycle|compaction/.test(event.kind))
      .map(codexEventRecord),
    toolEvents: events
      .filter((event) => /activity/.test(event.kind))
      .map(codexEventRecord),
    commands: events
      .filter((event) => event.kind === "command_activity")
      .map(codexEventRecord),
    fileChanges: events
      .filter((event) => event.kind === "file_activity")
      .map(codexEventRecord),
    tests: [
      { command: scenario.validationCommands[0], passed: input.supported },
    ],
    interactions: input.interactions ?? [],
    capabilities: scenario.requiredCapabilities.map((capability) => ({
      capability,
      support: input.supported ? "supported" : "unsupported",
      evidence: input.supported
        ? "native lifecycle evidence observed"
        : undefined,
      reason: input.supported
        ? undefined
        : "required lifecycle evidence missing",
    })),
    finalResponse: input.finalResponse,
    failures: input.supported
      ? []
      : [{ code: "lifecycle_validation_failed", message: input.finalResponse }],
    restart: input.restart ?? { exercised: false },
  };
}

function codexEventRecord(
  event: NeutralExternalRuntimeEvent,
): Record<string, unknown> {
  return {
    sequence: event.transportSequence,
    method: event.method,
    kind: event.kind,
    threadId: event.threadId,
    turnId: event.turnId,
    itemId: event.itemId,
    payload: event.payload,
  };
}

function unsupportedReason(
  applicability: CapabilityScenario["runtimeApplicability"]["codex_app_server"],
): string {
  assert.equal(applicability.status, "unsupported");
  return applicability.status === "unsupported"
    ? applicability.reason
    : "runtime capability is applicable";
}

function unsupportedEvidence(
  scenario: CapabilityScenario,
  runtimeKind: RuntimeEvidence["runtimeKind"],
  runtimeId: string,
  reason: string,
): RuntimeEvidence {
  const now = new Date().toISOString();
  return {
    runtimeId,
    runtimeKind,
    backend: "not-invoked",
    effectiveConfig: {},
    tools: [],
    startedAt: now,
    finishedAt: now,
    durationMs: 0,
    lifecycleEvents: [],
    toolEvents: [],
    commands: [],
    fileChanges: [],
    tests: [],
    interactions: [],
    capabilities: scenario.requiredCapabilities.map((capability) => ({
      capability,
      support: "unsupported",
      reason,
    })),
    failures: [],
    restart: { exercised: false },
  };
}

async function completeTextTurn(threadId: string, text: string): Promise<void> {
  const turn = await startTextTurn(threadId, text);
  await waitForTurnTerminal(authority, turn.turn.id);
}

function startTextTurn(threadId: string, text: string) {
  return driver.turnStart({
    threadId,
    input: [textInput(text)],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
  });
}

function textInput(text: string) {
  return { type: "text" as const, text, text_elements: [] };
}

async function waitForTurnTerminal(
  targetAuthority: RecordingCodexAuthority,
  turnId: string,
): Promise<void> {
  await waitForEvent(
    targetAuthority,
    (event) =>
      ["turn/completed", "turn/interrupted"].includes(event.method) &&
      event.turnId === turnId,
  );
}

async function waitForEvent(
  targetAuthority: RecordingCodexAuthority,
  predicate: (event: NeutralExternalRuntimeEvent) => boolean,
): Promise<NeutralExternalRuntimeEvent> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const event = targetAuthority.events.find(predicate);
    if (event) return event;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`native event was not observed within ${timeoutMs}ms`);
}

function assistantText(events: NeutralExternalRuntimeEvent[]): string {
  return events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => event.payload.text ?? "")
    .join("");
}

async function submitDirectTurn(text: string): Promise<{
  finalResponse: string;
  events: Array<{ kind: string; payload: Record<string, unknown> }>;
  tools: string[];
}> {
  const before = await serviceJson(
    `/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}?limit=1`,
  );
  const cursor = String(before.data.session.latest_cursor);
  const messageId = `${runId}-${randomUUID()}`;
  const response = await fetch(
    `${serviceBaseUrl}/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}/messages`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": messageId,
      },
      body: JSON.stringify({
        actor: { id: "capability-harness", kind: "human" },
        body: text,
        client_message_id: messageId,
        reason: "lifecycle capability certification",
      }),
      signal: AbortSignal.timeout(timeoutMs),
    },
  );
  assert.equal(response.status, 202, await response.text());
  const replay = await serviceJson(
    `/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
  );
  const events = replay.data.items as Array<{
    kind: string;
    payload: Record<string, unknown>;
  }>;
  const finalResponse = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => String(event.payload.text ?? ""))
    .join("");
  return {
    finalResponse,
    events,
    tools: [
      ...new Set(
        events
          .map((event) => event.payload.tool_name)
          .filter((value): value is string => typeof value === "string"),
      ),
    ],
  };
}

async function waitForServiceReady(): Promise<void> {
  await retry(async () => {
    const response = await fetch(`${serviceBaseUrl}/health`, {
      signal: AbortSignal.timeout(2_000),
    });
    if (!response.ok) throw new Error(`health returned ${response.status}`);
  });
}

async function serviceJson(path: string): Promise<any> {
  const response = await fetch(`${serviceBaseUrl}${path}`, {
    signal: AbortSignal.timeout(30_000),
  });
  const body = await response.json();
  assert.equal(response.status, 200, JSON.stringify(body));
  return body;
}

async function retry(operation: () => Promise<unknown>): Promise<void> {
  const deadline = Date.now() + 30_000;
  let lastError: unknown;
  while (Date.now() < deadline) {
    try {
      await operation();
      return;
    } catch (error) {
      lastError = error;
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  }
  throw lastError;
}

async function waitFor(predicate: () => boolean, label: string): Promise<void> {
  await retry(async () => {
    if (!predicate()) throw new Error(`${label} is not ready`);
  });
}
