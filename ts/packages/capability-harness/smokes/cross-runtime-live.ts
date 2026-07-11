import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join } from "node:path";

import {
  CODEX_APP_SERVER_PROTOCOL,
  CodexAppServerDriver,
  UnixWebSocketTransport,
  type CodexControllerAuthority,
  type CodexProtocolFault,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "@rusty-crew/external-runtime-codex";

import {
  CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  buildEvidenceComparison,
  validateCapabilityScenario,
  writeCapabilityArtifacts,
  type CapabilityEvidencePacket,
  type CapabilityObservation,
  type CapabilityScenario,
  type RuntimeEvidence,
} from "../src/index.js";

const serviceBaseUrl =
  process.env.RUSTY_CREW_CAPABILITY_SERVICE_URL ?? "http://127.0.0.1:9348";
const responsesSessionId =
  process.env.RUSTY_CREW_CAPABILITY_RESPONSES_SESSION ??
  "responses-cert-5389-session";
const socketPath =
  process.env.CODEX_APP_SERVER_SOCKET ??
  "/run/user/1001/codex-app-server/app-server.sock";
const turnTimeoutMs = Number(
  process.env.RUSTY_CREW_CAPABILITY_TURN_TIMEOUT_MS ?? 300_000,
);
const artifactRoot =
  process.env.RUSTY_CREW_CAPABILITY_ARTIFACT_ROOT ??
  `/tmp/rusty-crew-capability-${Date.now()}`;
const sharedScratchParent =
  process.env.RUSTY_CREW_CAPABILITY_SCRATCH_PARENT ?? "/home/.tmp";
mkdirSync(sharedScratchParent, { recursive: true });
const scratchRoot = mkdtempSync(
  join(sharedScratchParent, "rusty-crew-capability-fixtures-"),
);
const runId = `capability-${Date.now()}-${randomUUID().slice(0, 8)}`;

const scenarios = [
  validateCapabilityScenario({
    id: "focused_code_edit",
    title: "Focused code edit with validation",
    prompt:
      "Read value.json, change only its value field from before to after, then run node test.mjs. Report the validation result and marker CAPABILITY_EDIT_OK.",
    fixture: { kind: "directory", sourceRef: "fixture://focused-code-edit" },
    requiredCapabilities: ["file_write", "command_execution"],
    permittedEffects: ["fixture_repo_write", "fixture_command_execution"],
    expectedArtifacts: ["value.json"],
    validationCommands: ["node test.mjs"],
  }),
  validateCapabilityScenario({
    id: "structured_readback",
    title: "Structured readback after prior work",
    prompt:
      "Read value.json and reply with exactly CAPABILITY_READBACK_OK:after if its value is after. Do not modify files.",
    fixture: { kind: "directory", sourceRef: "fixture://focused-code-edit" },
    requiredCapabilities: ["file_read", "second_turn_continuation"],
    permittedEffects: ["fixture_read"],
    expectedArtifacts: ["assistant_response"],
    validationCommands: ["node test.mjs"],
  }),
] satisfies CapabilityScenario[];

interface RuntimeRun {
  evidence: RuntimeEvidence;
  raw: unknown;
}

class RecordingAuthority implements CodexControllerAuthority {
  readonly events: NeutralExternalRuntimeEvent[] = [];
  readonly faults: CodexProtocolFault[] = [];
  readonly interactions: Array<Record<string, unknown>> = [];

  async authorizeHandshake(identity: {
    userAgent: string;
    codexHome: string;
  }): Promise<{ accepted: boolean; message?: string }> {
    const accepted =
      identity.userAgent.includes(CODEX_APP_SERVER_PROTOCOL.cliVersion) &&
      identity.codexHome.length > 0;
    return accepted
      ? { accepted }
      : { accepted, message: `unexpected identity ${identity.userAgent}` };
  }

  hasControllerLease(): boolean {
    return true;
  }

  onEvent(event: NeutralExternalRuntimeEvent): void {
    this.events.push(event);
  }

  resolveServerRequest(
    context: Parameters<CodexControllerAuthority["resolveServerRequest"]>[0],
  ): Promise<ServerRequestResolution> {
    this.interactions.push({
      method: context.request.method,
      transportSequence: context.transportSequence,
    });
    return Promise.resolve({
      type: "error",
      code: -32000,
      message: `capability harness does not permit ${context.request.method}`,
    });
  }

  onProtocolFault(fault: CodexProtocolFault): void {
    this.faults.push(fault);
  }

  onDisconnected(): void {}
}

mkdirSync(artifactRoot, { recursive: true });
const codexFixture = createFixture("codex");
const responsesFixture = createFixture("responses");
const codexAuthority = new RecordingAuthority();
const codex = new CodexAppServerDriver(
  new UnixWebSocketTransport(socketPath),
  codexAuthority,
);
let codexThreadId: string | undefined;

try {
  const initialized = await codex.connect();
  const started = await codex.threadStart({
    cwd: codexFixture,
    approvalPolicy: "never",
    sandbox: "danger-full-access",
    ephemeral: false,
  });
  codexThreadId = started.thread.id;

  for (const scenario of scenarios) {
    const codexResult = await runCodexScenario(
      scenario,
      codex,
      codexAuthority,
      codexThreadId,
      codexFixture,
    );
    const responsesResult = await runResponsesScenario(
      scenario,
      responsesFixture,
    );
    const runtimes = [codexResult.evidence, responsesResult.evidence];
    const packet: CapabilityEvidencePacket = {
      schemaVersion: CAPABILITY_EVIDENCE_SCHEMA_VERSION,
      runId: `${runId}-${scenario.id}`,
      createdAt: new Date().toISOString(),
      scenario,
      runtimes,
      comparison: buildEvidenceComparison(runtimes),
    };
    await writeCapabilityArtifacts(join(artifactRoot, scenario.id), packet, {
      codex_app_server: codexResult.raw,
      direct_responses: responsesResult.raw,
      initialize: initialized,
    });
    assert.equal(
      Object.values(packet.comparison.scenarioPassedByRuntime).every(Boolean),
      true,
      `${scenario.id} must pass through both live runtime paths; inspect ${join(artifactRoot, scenario.id)}`,
    );
  }

  console.log(
    JSON.stringify(
      {
        runId,
        artifactRoot,
        scenarios: scenarios.map((scenario) => scenario.id),
        runtimes: ["codex_app_server", "direct_responses"],
        codexThreadId,
        responsesSessionId,
        exactProtocol: CODEX_APP_SERVER_PROTOCOL.cliVersion,
      },
      null,
      2,
    ),
  );
} finally {
  await codex.close().catch(() => undefined);
  rmSync(scratchRoot, { recursive: true, force: true });
}

async function runCodexScenario(
  scenario: CapabilityScenario,
  driver: CodexAppServerDriver,
  authority: RecordingAuthority,
  threadId: string,
  fixture: string,
): Promise<RuntimeRun> {
  const eventStart = authority.events.length;
  const startedAt = new Date();
  const turn = await driver.turnStart({
    threadId,
    input: [
      {
        type: "text",
        text: `${scenario.prompt}\n\nThe fixture directory is ${fixture}.`,
        text_elements: [],
      },
    ],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
  });
  await waitForCodexTerminal(authority, turn.turn.id);
  const finishedAt = new Date();
  const events = authority.events.slice(eventStart);
  const validation = validateFixture(fixture);
  const finalResponse = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => textValue(event.payload.text))
    .filter((value): value is string => value !== undefined)
    .join("");
  const capabilities = scenarioCapabilities(
    scenario,
    validation,
    finalResponse,
    events.some((event) => event.kind === "command_activity"),
  );
  const failures: Array<{ code: string; message: string }> = authority.faults
    .filter((fault) => fault.fatal)
    .map((fault) => ({ code: fault.reasonCode, message: fault.message }));
  if (!validation.passed) {
    failures.push({ code: "validation_failed", message: validation.output });
  }
  return {
    evidence: {
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
      effectiveConfig: {
        approvalPolicy: "never",
        sandbox: "danger-full-access",
        cwd: fixture,
        threadId,
        turnId: turn.turn.id,
      },
      tools: uniqueToolNames(events),
      startedAt: startedAt.toISOString(),
      finishedAt: finishedAt.toISOString(),
      durationMs: finishedAt.getTime() - startedAt.getTime(),
      lifecycleEvents: events
        .filter((event) => event.kind.endsWith("lifecycle"))
        .map(normalizedCodexEvent),
      toolEvents: events
        .filter((event) =>
          [
            "command_activity",
            "file_activity",
            "mcp_activity",
            "dynamic_tool_activity",
          ].includes(event.kind),
        )
        .map(normalizedCodexEvent),
      commands: events
        .filter((event) => event.kind === "command_activity")
        .map(normalizedCodexEvent),
      fileChanges: events
        .filter((event) => event.kind === "file_activity")
        .map(normalizedCodexEvent),
      tests: [validation],
      interactions: [...authority.interactions],
      capabilities,
      finalResponse,
      failures,
      restart: { exercised: false },
    },
    raw: { events, faults: authority.faults, validation },
  };
}

async function runResponsesScenario(
  scenario: CapabilityScenario,
  fixture: string,
): Promise<RuntimeRun> {
  const before = await getJson(
    `/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}?limit=1`,
  );
  const cursor = String(before.data.session.latest_cursor);
  const clientMessageId = `${runId}-${scenario.id}-responses`;
  const startedAt = new Date();
  const response = await fetch(
    `${serviceBaseUrl}/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}/messages`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": clientMessageId,
      },
      body: JSON.stringify({
        actor: { id: "capability-harness", kind: "human" },
        body: `${scenario.prompt}\n\nThe fixture directory is ${fixture}.`,
        client_message_id: clientMessageId,
        reason: `cross-runtime capability ${scenario.id}`,
      }),
      signal: AbortSignal.timeout(turnTimeoutMs),
    },
  );
  const responseBody = (await response.json()) as Record<string, unknown>;
  assert.equal(response.status, 202, JSON.stringify(responseBody));
  const replay = await getJson(
    `/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
  );
  const events = replay.data.items as Array<{
    kind: string;
    payload: Record<string, unknown>;
    [key: string]: unknown;
  }>;
  const finishedAt = new Date();
  const validation = validateFixture(fixture);
  const finalResponse = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => textValue(event.payload.text))
    .filter((value): value is string => value !== undefined)
    .join("");
  const failures = events
    .filter((event) =>
      ["stream_error", "tool_call_failed", "command_failed"].includes(
        event.kind,
      ),
    )
    .map((event) => ({
      code: event.kind,
      message: JSON.stringify(event.payload),
    }));
  if (!validation.passed) {
    failures.push({ code: "validation_failed", message: validation.output });
  }
  const capabilities = scenarioCapabilities(
    scenario,
    validation,
    finalResponse,
    events.some(
      (event) =>
        event.kind.startsWith("command_") ||
        event.payload.tool_name === "terminal",
    ),
  );
  return {
    evidence: {
      runtimeId: "direct-responses",
      runtimeKind: "direct_brain",
      backend: serviceBaseUrl,
      model: "gpt",
      effort: "medium",
      effectiveConfig: {
        sessionId: responsesSessionId,
        profileId: "responses-cert-5389",
        providerAlias: "responses-proxy-cert-5389",
        cwd: fixture,
      },
      tools: uniqueChatToolNames(events),
      startedAt: startedAt.toISOString(),
      finishedAt: finishedAt.toISOString(),
      durationMs: finishedAt.getTime() - startedAt.getTime(),
      lifecycleEvents: events.filter((event) =>
        ["assistant_turn_started", "assistant_turn_finished"].includes(
          event.kind,
        ),
      ),
      toolEvents: events.filter((event) =>
        /^(?:tool_call|command)_/.test(event.kind),
      ),
      commands: events.filter(
        (event) =>
          event.kind.startsWith("command_") ||
          event.payload.tool_name === "terminal",
      ),
      fileChanges:
        validation.passed &&
        scenario.requiredCapabilities.includes("file_write")
          ? [{ path: join(fixture, "value.json"), observedValue: "after" }]
          : [],
      tests: [validation],
      interactions: [],
      capabilities,
      finalResponse,
      failures,
      restart: { exercised: false },
    },
    raw: { response: responseBody, events, validation },
  };
}

async function waitForCodexTerminal(
  authority: RecordingAuthority,
  turnId: string,
): Promise<void> {
  const deadline = Date.now() + turnTimeoutMs;
  while (Date.now() < deadline) {
    if (
      authority.events.some(
        (event) => event.method === "turn/completed" && event.turnId === turnId,
      )
    ) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`Codex turn ${turnId} exceeded ${turnTimeoutMs}ms`);
}

function createFixture(label: string): string {
  const directory = join(scratchRoot, label);
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, "value.json"), '{"value":"before"}\n');
  writeFileSync(
    join(directory, "test.mjs"),
    [
      'import assert from "node:assert/strict";',
      'import { readFileSync } from "node:fs";',
      'const value = JSON.parse(readFileSync(new URL("./value.json", import.meta.url), "utf8"));',
      'assert.equal(value.value, "after");',
      'console.log("FIXTURE_TEST_OK");',
      "",
    ].join("\n"),
  );
  return directory;
}

function validateFixture(directory: string): Record<string, unknown> & {
  passed: boolean;
  output: string;
} {
  try {
    const output = execFileSync(process.execPath, ["test.mjs"], {
      cwd: directory,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
    const stored = JSON.parse(
      readFileSync(join(directory, "value.json"), "utf8"),
    ) as { value?: unknown };
    return {
      command: "node test.mjs",
      passed: stored.value === "after" && output.includes("FIXTURE_TEST_OK"),
      output,
      value: stored.value,
    };
  } catch (error) {
    return {
      command: "node test.mjs",
      passed: false,
      output: error instanceof Error ? error.message : String(error),
    };
  }
}

function scenarioCapabilities(
  scenario: CapabilityScenario,
  validation: { passed: boolean },
  finalResponse: string,
  commandObserved: boolean,
): CapabilityObservation[] {
  return scenario.requiredCapabilities.map((capability) => {
    let supported = false;
    if (capability === "file_write") supported = validation.passed;
    if (capability === "command_execution") {
      supported = validation.passed && commandObserved;
    }
    if (capability === "file_read") {
      supported = finalResponse.includes("CAPABILITY_READBACK_OK:after");
    }
    if (capability === "second_turn_continuation") {
      supported = finalResponse.includes("CAPABILITY_READBACK_OK:after");
    }
    return supported
      ? { capability, support: "supported", evidence: "live scenario passed" }
      : {
          capability,
          support: "unsupported",
          reason: "required live evidence was not observed",
        };
  });
}

function normalizedCodexEvent(
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

function uniqueToolNames(
  events: readonly NeutralExternalRuntimeEvent[],
): string[] {
  return [
    ...new Set(
      events
        .filter((event) =>
          ["command_activity", "file_activity", "mcp_activity"].includes(
            event.kind,
          ),
        )
        .map((event) => event.kind),
    ),
  ];
}

function uniqueChatToolNames(
  events: Array<{ kind: string; payload: Record<string, unknown> }>,
): string[] {
  return [
    ...new Set(
      events
        .map((event) => textValue(event.payload.tool_name))
        .filter((value): value is string => value !== undefined),
    ),
  ];
}

async function getJson(path: string): Promise<any> {
  const response = await fetch(`${serviceBaseUrl}${path}`, {
    signal: AbortSignal.timeout(30_000),
  });
  const body = await response.json();
  assert.equal(response.status, 200, JSON.stringify(body));
  assert.equal(body.ok, true, JSON.stringify(body));
  return body;
}

function textValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}
