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
  type NeutralExternalRuntimeEvent,
} from "@rusty-crew/external-runtime-codex";

import {
  CAPABILITY_EVIDENCE_SCHEMA_VERSION,
  RecordingCodexAuthority,
  buildEvidenceComparison,
  expandedCapabilityScenarios,
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

const scenarios = expandedCapabilityScenarios;

interface RuntimeRun {
  evidence: RuntimeEvidence;
  raw: unknown;
}

mkdirSync(artifactRoot, { recursive: true });
const codexFixture = createFixture("codex");
const responsesFixture = createFixture("responses");
const codexAuthority = new RecordingCodexAuthority();
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
    const codexResult =
      scenario.runtimeApplicability.codex_app_server.status === "applicable"
        ? await runCodexScenario(
            scenario,
            codex,
            codexAuthority,
            codexThreadId,
            codexFixture,
          )
        : unsupportedRuntimeRun(
            scenario,
            "codex_app_server",
            "codex-app-server",
            scenario.runtimeApplicability.codex_app_server.reason,
          );
    const responsesResult =
      scenario.runtimeApplicability.direct_brain.status === "applicable"
        ? await runResponsesScenario(scenario, responsesFixture)
        : unsupportedRuntimeRun(
            scenario,
            "direct_brain",
            "direct-responses",
            scenario.runtimeApplicability.direct_brain.reason,
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
    if (scenario.id === "multi_file_repo_instructions") {
      assert.equal(
        Object.values(packet.comparison.scenarioPassedByRuntime).every(Boolean),
        true,
        `${scenario.id} must pass through both live runtime paths; inspect ${join(artifactRoot, scenario.id)}`,
      );
    }
    if (scenario.id === "den_mcp_read_write") {
      assert.equal(
        packet.comparison.scenarioPassedByRuntime["codex-app-server"],
        true,
        `Codex must complete the live Den workflow; inspect ${join(artifactRoot, scenario.id)}`,
      );
    }
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
  authority: RecordingCodexAuthority,
  threadId: string,
  fixture: string,
): Promise<RuntimeRun> {
  const eventStart = authority.events.length;
  const interactionStart = authority.interactions.length;
  const faultStart = authority.faults.length;
  const startedAt = new Date();
  const prompt = renderScenarioPrompt(scenario, "codex-app-server");
  const input: Parameters<CodexAppServerDriver["turnStart"]>[0]["input"] = [
    {
      type: "text",
      text: `${prompt}\n\nThe fixture directory is ${fixture}.`,
      text_elements: [],
    },
  ];
  if (scenario.id === "local_visual_input") {
    input.push({ type: "localImage", path: join(fixture, "red-square.png") });
  }
  const turn = await driver.turnStart({
    threadId,
    input,
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
  });
  await waitForCodexTerminal(authority, turn.turn.id);
  const finishedAt = new Date();
  const events = authority.events.slice(eventStart);
  const finalResponse = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => textValue(event.payload.text))
    .filter((value): value is string => value !== undefined)
    .join("");
  const validation = validateScenarioFixture(scenario, fixture, finalResponse);
  const capabilities = scenarioCapabilities(
    scenario,
    validation,
    finalResponse,
    codexObservedCapabilities(events),
  );
  const faults = authority.faults.slice(faultStart);
  const failures: Array<{ code: string; message: string }> = faults
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
      tests: validation.tests,
      interactions: authority.interactions.slice(interactionStart),
      capabilities,
      finalResponse,
      failures,
      restart: { exercised: false },
    },
    raw: { events, faults, validation },
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
        body: `${renderScenarioPrompt(scenario, "direct-responses")}\n\nThe fixture directory is ${fixture}.`,
        client_message_id: clientMessageId,
        reason: `cross-runtime capability ${scenario.id}`,
      }),
      signal: AbortSignal.timeout(turnTimeoutMs),
    },
  );
  const responseBody = (await response.json()) as Record<string, unknown>;
  assert.equal(response.status, 202, JSON.stringify(responseBody));
  const events = await readResponsesEventsUntilTerminal(cursor);
  const finishedAt = new Date();
  const finalResponse = events
    .filter((event) => event.kind === "assistant_text_delta")
    .map((event) => textValue(event.payload.text))
    .filter((value): value is string => value !== undefined)
    .join("");
  const validation = validateScenarioFixture(scenario, fixture, finalResponse);
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
    directObservedCapabilities(events),
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
      fileChanges: validation.fileChanges,
      tests: validation.tests,
      interactions: [],
      capabilities,
      finalResponse,
      failures,
      restart: { exercised: false },
    },
    raw: { response: responseBody, events, validation },
  };
}

type ChatReplayEvent = {
  kind: string;
  payload: Record<string, unknown>;
  [key: string]: unknown;
};

async function readResponsesEventsUntilTerminal(
  initialCursor: string,
): Promise<ChatReplayEvent[]> {
  const events: ChatReplayEvent[] = [];
  let cursor = initialCursor;
  const deadline = Date.now() + turnTimeoutMs;
  while (Date.now() < deadline) {
    const replay = await getJson(
      `/v1/chat/sessions/${encodeURIComponent(responsesSessionId)}/events?cursor=${encodeURIComponent(cursor)}&limit=500`,
    );
    const page = replay.data.items as ChatReplayEvent[];
    events.push(...page);
    cursor = String(replay.data.latest_cursor ?? cursor);
    if (
      events.some((event) =>
        ["assistant_turn_finished", "assistant_message_completed"].includes(
          event.kind,
        ),
      ) &&
      replay.data.has_more !== true
    ) {
      return events;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`direct Responses turn exceeded ${turnTimeoutMs}ms`);
}

async function waitForCodexTerminal(
  authority: RecordingCodexAuthority,
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
  mkdirSync(join(directory, "src"), { recursive: true });
  writeFileSync(
    join(directory, "AGENTS.md"),
    [
      "# Fixture Instructions",
      "",
      "For the multi-file task, set config.json mode to certified and",
      "replace src/component.txt with component-certified.",
      "Always run node multi-test.mjs after those edits.",
      "",
    ].join("\n"),
  );
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
  writeFileSync(join(directory, "config.json"), '{"mode":"before"}\n');
  writeFileSync(join(directory, "src/component.txt"), "component-before\n");
  writeFileSync(
    join(directory, "multi-test.mjs"),
    [
      'import assert from "node:assert/strict";',
      'import { readFileSync } from "node:fs";',
      'const config = JSON.parse(readFileSync(new URL("./config.json", import.meta.url), "utf8"));',
      'const component = readFileSync(new URL("./src/component.txt", import.meta.url), "utf8").trim();',
      'assert.equal(config.mode, "certified");',
      'assert.equal(component, "component-certified");',
      'console.log("MULTI_FILE_TEST_OK");',
      "",
    ].join("\n"),
  );
  writeFileSync(join(directory, "delegate.txt"), "delegated-evidence\n");
  writeFileSync(
    join(directory, "background-test.mjs"),
    [
      'import assert from "node:assert/strict";',
      'import { readFileSync } from "node:fs";',
      'assert.equal(readFileSync(new URL("./background.txt", import.meta.url), "utf8").trim(), "BACKGROUND_OK");',
      'console.log("BACKGROUND_TEST_OK");',
      "",
    ].join("\n"),
  );
  writeFileSync(
    join(directory, "red-square.png"),
    Buffer.from(
      "iVBORw0KGgoAAAANSUhEUgAAAEAAAABAAQMAAACQp+OdAAAAIGNIUk0AAHomAACAhAAA+gAAAIDoAAB1MAAA6mAAADqYAAAXcJy6UTwAAAAGUExURf8AAP///0EdNBEAAAABYktHRAH/Ai3eAAAAB3RJTUUH6gcLCgIXolirxgAAACV0RVh0ZGF0ZTpjcmVhdGUAMjAyNi0wNy0xMVQxMDowMjoyMyswMDowMPeqwUAAAAAldEVYdGRhdGU6bW9kaWZ5ADIwMjYtMDctMTFUMTA6MDI6MjMrMDA6MDCG93n8AAAAKHRFWHRkYXRlOnRpbWVzdGFtcAAyMDI2LTA3LTExVDEwOjAyOjIzKzAwOjAw0eJYIwAAAA9JREFUKM9jYBgFo4B8AAACQAABjMWrdwAAAABJRU5ErkJggg==",
      "base64",
    ),
  );
  return directory;
}

interface ScenarioValidation {
  passed: boolean;
  output: string;
  tests: Array<Record<string, unknown>>;
  fileChanges: Array<Record<string, unknown>>;
}

function validateScenarioFixture(
  scenario: CapabilityScenario,
  directory: string,
  finalResponse: string,
): ScenarioValidation {
  const tests = scenario.validationCommands
    .filter((command) => command.startsWith("node "))
    .map((command) => runFixtureCommand(directory, command));
  let passed = tests.every((test) => test.passed === true);
  const fileChanges: Array<Record<string, unknown>> = [];
  if (scenario.id === "focused_code_edit") {
    const stored = JSON.parse(
      readFileSync(join(directory, "value.json"), "utf8"),
    ) as { value?: unknown };
    passed &&= stored.value === "after";
    if (stored.value === "after") {
      fileChanges.push({ path: join(directory, "value.json") });
    }
  } else if (scenario.id === "structured_readback") {
    passed &&= finalResponse.includes("CAPABILITY_READBACK_OK:after");
  } else if (scenario.id === "multi_file_repo_instructions") {
    const config = JSON.parse(
      readFileSync(join(directory, "config.json"), "utf8"),
    ) as { mode?: unknown };
    const component = readFileSync(
      join(directory, "src/component.txt"),
      "utf8",
    ).trim();
    passed &&=
      config.mode === "certified" && component === "component-certified";
    if (passed) {
      fileChanges.push(
        { path: join(directory, "config.json") },
        { path: join(directory, "src/component.txt") },
      );
    }
  } else if (scenario.id === "den_mcp_read_write") {
    passed &&= finalResponse.includes(
      `CAPABILITY_DEN_WRITE_${runId}_codex-app-server`,
    );
  } else if (scenario.id === "web_tool_use") {
    passed &&=
      finalResponse.includes("CAPABILITY_WEB_OK") &&
      /Example Domain/i.test(finalResponse);
  } else if (scenario.id === "background_command") {
    passed &&= finalResponse.includes("CAPABILITY_BACKGROUND_OK");
    if (passed) fileChanges.push({ path: join(directory, "background.txt") });
  } else if (scenario.id === "local_visual_input") {
    passed &&= finalResponse.includes("CAPABILITY_IMAGE_OK:red");
  } else if (scenario.id === "subagent_delegation") {
    passed &&= finalResponse.includes(
      "CAPABILITY_SUBAGENT_OK:delegated-evidence",
    );
  }
  return {
    passed,
    output: tests.map((test) => String(test.output ?? "")).join("\n"),
    tests:
      tests.length > 0
        ? tests
        : [{ command: scenario.validationCommands[0], passed }],
    fileChanges,
  };
}

function runFixtureCommand(
  directory: string,
  command: string,
): Record<string, unknown> {
  try {
    const output = execFileSync("/bin/bash", ["-c", command], {
      cwd: directory,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
    return { command, passed: true, output };
  } catch (error) {
    return {
      command,
      passed: false,
      output: error instanceof Error ? error.message : String(error),
    };
  }
}

function scenarioCapabilities(
  scenario: CapabilityScenario,
  validation: { passed: boolean },
  finalResponse: string,
  observed: ReadonlySet<string>,
): CapabilityObservation[] {
  return scenario.requiredCapabilities.map((capability) => {
    let supported = false;
    if (capability === "file_write") supported = validation.passed;
    if (capability === "command_execution") {
      supported = validation.passed && observed.has("command_execution");
    }
    if (capability === "file_read") {
      supported =
        finalResponse.includes("CAPABILITY_READBACK_OK:after") ||
        finalResponse.includes("CAPABILITY_BACKGROUND_OK");
    }
    if (capability === "second_turn_continuation") {
      supported = finalResponse.includes("CAPABILITY_READBACK_OK:after");
    }
    if (capability === "repo_instruction_discovery")
      supported = validation.passed;
    if (capability === "multi_file_write") supported = validation.passed;
    if (capability === "den_mcp_read" || capability === "den_mcp_write") {
      supported = validation.passed && observed.has("mcp_activity");
    }
    if (capability === "web_access" || capability === "external_source_read") {
      supported = validation.passed && observed.has("web_activity");
    }
    if (capability === "background_command") {
      supported = validation.passed && observed.has("command_execution");
    }
    if (capability === "local_visual_input") supported = validation.passed;
    if (
      capability === "subagent_delegation" ||
      capability === "delegated_result"
    ) {
      supported = validation.passed && observed.has("delegation_activity");
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

function renderScenarioPrompt(
  scenario: CapabilityScenario,
  runtimeId: string,
): string {
  return scenario.prompt
    .replaceAll("{{RUN_ID}}", runId)
    .replaceAll("{{RUNTIME_ID}}", runtimeId);
}

function codexObservedCapabilities(
  events: readonly NeutralExternalRuntimeEvent[],
): Set<string> {
  const observed = new Set<string>();
  for (const event of events) {
    const searchable =
      `${event.method} ${event.payload.server ?? ""} ${event.payload.tool ?? ""} ${event.rawDetail.json}`.toLowerCase();
    if (event.kind === "command_activity") observed.add("command_execution");
    if (event.kind === "mcp_activity") observed.add("mcp_activity");
    if (
      event.kind === "mcp_activity" ||
      /web[_ -]?search|web[_ -]?extract|browser|example\.com/.test(searchable)
    ) {
      observed.add("web_activity");
    }
    if (/collab|subagent|spawn_agent|delegate/.test(searchable)) {
      observed.add("delegation_activity");
    }
  }
  return observed;
}

function directObservedCapabilities(
  events: Array<{ kind: string; payload: Record<string, unknown> }>,
): Set<string> {
  const observed = new Set<string>();
  for (const event of events) {
    const toolName = textValue(event.payload.tool_name)?.toLowerCase() ?? "";
    if (event.kind.startsWith("command_") || toolName === "terminal") {
      observed.add("command_execution");
    }
    if (toolName.includes("mcp") || toolName.startsWith("env_")) {
      observed.add("mcp_activity");
    }
    if (/web_search|web_extract|browser/.test(toolName)) {
      observed.add("web_activity");
    }
    if (/spawn_subagent|fan_out_subagents|scout_codebase/.test(toolName)) {
      observed.add("delegation_activity");
    }
  }
  return observed;
}

function unsupportedRuntimeRun(
  scenario: CapabilityScenario,
  runtimeKind: RuntimeEvidence["runtimeKind"],
  runtimeId: string,
  reason: string,
): RuntimeRun {
  const now = new Date().toISOString();
  return {
    evidence: {
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
    },
    raw: { unsupported: true, reason },
  };
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
