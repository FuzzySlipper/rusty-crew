import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  CODEX_APP_SERVER_PROTOCOL,
  CODEX_COORDINATION_DYNAMIC_TOOLS,
  CodexAppServerDriver,
  UnixWebSocketTransport,
  type CodexControllerAuthority,
  type CodexProtocolFault,
  type DynamicToolSpec,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "../src/index.js";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../../../..");
const socketPath =
  process.env.CODEX_APP_SERVER_SOCKET ??
  "/run/user/1001/codex-app-server/app-server.sock";
const timeoutMs = Number(
  process.env.CODEX_APP_SERVER_LIVE_TIMEOUT_MS ?? 300_000,
);
const scratch =
  process.env.CODEX_APP_SERVER_DRIVER_SMOKE_ROOT ??
  `/tmp/rusty-crew-codex-driver-${Date.now()}`;
const token = randomUUID();
const PROFILE_MCP_NAMESPACE_PROBE: DynamicToolSpec = {
  type: "namespace",
  name: "rusty_crew_mcp",
  description: "Rusty Crew exact-session profile MCP namespace probe",
  tools: [
    {
      type: "function",
      name: "den__get_task",
      description: "Profile MCP namespace acceptance probe",
      inputSchema: {
        type: "object",
        properties: { task_id: { type: "integer" } },
        required: ["task_id"],
        additionalProperties: false,
      },
    },
  ],
};

class LiveAuthority implements CodexControllerAuthority {
  readonly events: NeutralExternalRuntimeEvent[] = [];
  readonly faults: CodexProtocolFault[] = [];
  readonly dynamicCalls: string[] = [];
  readonly disconnects: string[] = [];

  async authorizeHandshake(identity: {
    userAgent: string;
    codexHome: string;
  }): Promise<{ accepted: boolean; message?: string }> {
    const accepted =
      identity.userAgent.includes(CODEX_APP_SERVER_PROTOCOL.cliVersion) &&
      identity.codexHome.length > 0;
    return {
      accepted,
      ...(accepted
        ? {}
        : { message: `unexpected initialize identity ${identity.userAgent}` }),
    };
  }

  hasControllerLease(): boolean {
    return true;
  }

  onEvent(event: NeutralExternalRuntimeEvent): void {
    this.events.push(event);
  }

  async resolveServerRequest(
    context: Parameters<CodexControllerAuthority["resolveServerRequest"]>[0],
  ): Promise<ServerRequestResolution> {
    if (context.request.method !== "item/tool/call") {
      return {
        type: "error",
        code: -32000,
        message: `live compatibility smoke declines ${context.request.method}`,
      };
    }
    const params = context.request.params;
    if (
      params.namespace !== "rusty_crew" ||
      params.tool !== "send_agent_message" ||
      typeof params.arguments !== "object" ||
      params.arguments === null ||
      !("recipient" in params.arguments) ||
      params.arguments.recipient !== "smoke-recipient" ||
      !("body" in params.arguments) ||
      params.arguments.body !== token
    ) {
      return {
        type: "error",
        code: -32602,
        message: "dynamic tool identity or token mismatch",
      };
    }
    this.dynamicCalls.push(params.callId);
    return {
      type: "success",
      result: {
        contentItems: [
          { type: "inputText", text: `RUSTY_CREW_DRIVER_ACK:${token}` },
        ],
        success: true,
      },
    };
  }

  onProtocolFault(fault: CodexProtocolFault): void {
    this.faults.push(fault);
  }

  onDisconnected(details: { reason: string }): void {
    this.disconnects.push(details.reason);
  }
}

async function waitForTurn(
  authority: LiveAuthority,
  turnId: string,
): Promise<NeutralExternalRuntimeEvent> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const terminal = authority.events.find(
      (event) => event.method === "turn/completed" && event.turnId === turnId,
    );
    if (terminal !== undefined) return terminal;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`turn ${turnId} did not complete after ${timeoutMs}ms`);
}

mkdirSync(scratch, { recursive: true });
let stage = "runtime protocol verification";
try {
  execFileSync(
    process.execPath,
    ["tools/generate-codex-app-server-protocol.mjs", "--check", "--runtime"],
    { cwd: root, stdio: "inherit" },
  );

  stage = "first controller connect";
  const firstAuthority = new LiveAuthority();
  const first = new CodexAppServerDriver(
    new UnixWebSocketTransport(socketPath),
    firstAuthority,
  );
  const initialized = await first.connect();
  stage = "persistent thread start";
  const started = await first.threadStart({
    cwd: scratch,
    approvalPolicy: "never",
    sandbox: "danger-full-access",
    ephemeral: false,
    dynamicTools: [
      ...CODEX_COORDINATION_DYNAMIC_TOOLS,
      PROFILE_MCP_NAMESPACE_PROBE,
    ],
  });
  stage = "dynamic-tool turn start";
  const turn = await first.turnStart({
    threadId: started.thread.id,
    input: [
      {
        type: "text",
        text: `Call rusty_crew.send_agent_message exactly once with recipient smoke-recipient and body ${token}, then reply with its acknowledgement.`,
        text_elements: [],
      },
    ],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
  });
  stage = "dynamic-tool turn completion";
  const terminal = await waitForTurn(firstAuthority, turn.turn.id);
  assert.equal(firstAuthority.dynamicCalls.length, 1);
  assert.equal(
    firstAuthority.faults.some((fault) => fault.fatal),
    false,
  );

  const checkpointPath = resolve(scratch, "rust-controller-checkpoint.json");
  writeFileSync(
    checkpointPath,
    JSON.stringify({
      nativeThreadId: started.thread.id,
      nativeTurnId: turn.turn.id,
      protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
    }),
  );
  stage = "first controller close";
  await first.close();

  const checkpoint = JSON.parse(readFileSync(checkpointPath, "utf8")) as {
    nativeThreadId: string;
    nativeTurnId: string;
    protocolSchemaSha256: string;
  };
  assert.equal(
    checkpoint.protocolSchemaSha256,
    CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
  );
  stage = "second controller connect";
  const resumedAuthority = new LiveAuthority();
  const resumed = new CodexAppServerDriver(
    new UnixWebSocketTransport(socketPath),
    resumedAuthority,
  );
  await resumed.connect();
  stage = "exact thread resume";
  const resumeResult = await resumed.threadResume({
    threadId: checkpoint.nativeThreadId,
    excludeTurns: true,
  });
  stage = "exact thread readback";
  const readback = await resumed.threadRead({
    threadId: checkpoint.nativeThreadId,
    includeTurns: true,
  });
  assert.equal(resumeResult.thread.id, checkpoint.nativeThreadId);
  assert.equal(readback.thread.id, checkpoint.nativeThreadId);
  assert.equal(
    readback.thread.turns.some(
      (candidate) => candidate.id === checkpoint.nativeTurnId,
    ),
    true,
  );
  await resumed.close();

  console.log(
    JSON.stringify(
      {
        socketPath,
        userAgent: initialized.userAgent,
        protocolSchemaSha256: CODEX_APP_SERVER_PROTOCOL.protocolSchemaSha256,
        threadId: checkpoint.nativeThreadId,
        turnId: checkpoint.nativeTurnId,
        terminalKind: terminal.kind,
        dynamicToolCalls: firstAuthority.dynamicCalls.length,
        exactThreadResume: true,
        fatalProtocolFaults: [
          ...firstAuthority.faults,
          ...resumedAuthority.faults,
        ].filter((fault) => fault.fatal).length,
      },
      null,
      2,
    ),
  );
} catch (error) {
  const detail =
    error instanceof Error
      ? (error.stack ?? error.message)
      : `non-Error rejection: ${JSON.stringify(error)}`;
  throw new Error(`Codex driver live smoke failed during ${stage}: ${detail}`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}
