import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ChatEvent } from "./rusty-view-chat-api.js";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-responses-field-"));
const port = await openPort();
writeRuntimeConfig(root);
let host = await startHost(root, port);

try {
  const client = createDebugApiClient({
    baseUrl: `http://127.0.0.1:${port}`,
    retries: 0,
    timeoutMs:
      Number(process.env.RUSTY_CREW_OPENAI_RESPONSES_SMOKE_TIMEOUT_MS) ||
      (process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE === "1" ? 60_000 : 2_000),
  });

  const initialProviderState = await providerStateDiagnostics();
  assert.equal(initialProviderState.providerState.status, "missing");
  assert.equal(initialProviderState.providerStateMode, "optional");
  assert.equal(initialProviderState.moduleId, "openai-responses");

  const beforeFirstTurn = await client.diagnostics();
  const firstTurn = await client.requestDirectDebugTurn({
    sessionId: "responses-session",
    actorId: "local-operator",
    body: "Exercise Responses replay service profile.",
    reason: "responses service field smoke",
  });
  assert.equal(firstTurn.status, "accepted");
  assert.match(firstTurn.summary, /responses replay (service )?wake completed/);
  assert.match(firstTurn.wakeId ?? "", /^service-responses-session-/);

  const afterFirstTurn = await client.diagnostics();
  assert.equal(
    completionPacketCount(afterFirstTurn),
    completionPacketCount(beforeFirstTurn) + 1,
  );
  const firstState = await providerStateDiagnostics();
  assert.equal(firstState.providerState.status, "valid");
  assert.equal(
    firstState.providerState.sessions[0]?.lastWakeId,
    firstTurn.wakeId,
  );
  assert.equal(
    firstState.providerState.sessions[0]?.payloadVersion,
    "openai-responses-state-v1",
  );

  const firstEvents = await chatEvents();
  assertChatKinds(firstEvents, expectedChatKinds());

  await host.stop();
  host = await startHost(root, port);

  const restartedState = await providerStateDiagnostics();
  assert.equal(restartedState.providerState.status, "valid");
  assert.equal(
    restartedState.providerState.sessions[0]?.lastWakeId,
    firstTurn.wakeId,
  );

  const beforeSecondTurn = await client.diagnostics();
  const secondTurn = await client.requestDirectDebugTurn({
    sessionId: "responses-session",
    actorId: "local-operator",
    body: "Exercise Responses replay after restart.",
    reason: "responses service restart field smoke",
  });
  assert.equal(secondTurn.status, "accepted");
  assert.match(
    secondTurn.summary,
    /responses replay (service )?wake completed/,
  );

  const afterSecondTurn = await client.diagnostics();
  assert.equal(
    completionPacketCount(afterSecondTurn),
    completionPacketCount(beforeSecondTurn) + 1,
  );
  const secondState = await providerStateDiagnostics();
  assert.equal(secondState.providerState.status, "valid");
  assert.equal(
    secondState.providerState.sessions[0]?.lastWakeId,
    secondTurn.wakeId,
  );

  const secondEvents = await chatEvents();
  assertChatKinds(secondEvents, expectedChatKinds());

  console.log(
    JSON.stringify(
      {
        profile: "responses-profile",
        session: "responses-session",
        route: "/v1/debug/sessions/responses-session/turn",
        diagnosticsRoute: "/v1/admin/diagnostics/provider-state",
        firstWakeId: firstTurn.wakeId,
        secondWakeId: secondTurn.wakeId,
        providerStateStatus: secondState.providerState.status,
        providerStateLastWakeId:
          secondState.providerState.sessions[0]?.lastWakeId,
        chatKinds: [...new Set(secondEvents.map((event) => event.kind))],
        liveProvider: process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE === "1",
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

function expectedChatKinds(): readonly ChatEvent["kind"][] {
  if (process.env.RUSTY_CREW_OPENAI_RESPONSES_LIVE === "1") {
    return [
      "assistant_turn_started",
      "assistant_text_delta",
      "assistant_message_completed",
      "assistant_turn_finished",
    ];
  }
  return [
    "assistant_turn_started",
    "tool_call_started",
    "tool_call_completed",
    "assistant_text_delta",
    "assistant_message_completed",
    "assistant_turn_finished",
  ];
}

async function providerStateDiagnostics(): Promise<{
  profileId: string;
  moduleId: string;
  providerStateMode: string;
  providerState: {
    status: string;
    sessions: Array<{
      status: string;
      lastWakeId?: string;
      payloadVersion?: string;
    }>;
  };
}> {
  const response = await fetch(
    `http://127.0.0.1:${port}/v1/admin/diagnostics/provider-state`,
  );
  assert.equal(response.status, 200);
  const envelope = (await response.json()) as {
    ok: boolean;
    data: Array<{
      profileId: string;
      moduleId: string;
      providerStateMode: string;
      providerState: {
        status: string;
        sessions: Array<{
          status: string;
          lastWakeId?: string;
          payloadVersion?: string;
        }>;
      };
    }>;
  };
  assert.equal(envelope.ok, true);
  const state = envelope.data.find(
    (candidate) => candidate.profileId === "responses-profile",
  );
  assert.ok(state, "responses profile provider-state diagnostics should exist");
  return state;
}

async function chatEvents(): Promise<ChatEvent[]> {
  const response = await fetch(
    `http://127.0.0.1:${port}/v1/chat/sessions/responses-session/events?limit=100`,
  );
  assert.equal(response.status, 200);
  const envelope = (await response.json()) as {
    ok: boolean;
    data: { items: ChatEvent[] };
  };
  assert.equal(envelope.ok, true);
  return envelope.data.items;
}

function assertChatKinds(
  events: readonly ChatEvent[],
  expectedKinds: readonly ChatEvent["kind"][],
): void {
  const kinds = new Set(events.map((event) => event.kind));
  for (const kind of expectedKinds) {
    assert.equal(kinds.has(kind), true, `expected chat event kind ${kind}`);
  }
}

function completionPacketCount(input: {
  overview: {
    persistence?: {
      tableCounts?: {
        completion_packets?: number;
      };
    };
  };
}): number {
  return input.overview.persistence?.tableCounts?.completion_packets ?? 0;
}

async function startHost(rootDir: string, hostPort: number) {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: rootDir,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(hostPort),
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "60000",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "50",
    },
  });
}

function writeRuntimeConfig(rootDir: string): void {
  const configDir = join(rootDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [
          {
            profileId: "responses-profile",
            implementationId: "responses-brain",
          },
        ],
        sessions: [
          {
            sessionId: "responses-session",
            agentId: "responses-agent",
            profileId: "responses-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "responses-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-profile",
        displayName: "Responses Field Smoke",
        modelConfig: {
          provider: "openai",
          modelName: process.env.RUSTY_CREW_OPENAI_RESPONSES_MODEL ?? "gpt-5",
          baseUrl: process.env.RUSTY_CREW_OPENAI_RESPONSES_BASE_URL,
          apiKeyEnv: process.env.RUSTY_CREW_OPENAI_RESPONSES_API_KEY_ENV,
          api: "responses",
        },
        brain: {
          module: "openai-responses",
          strategy: "replay",
        },
        toolPolicy: {
          requestedTools: ["read_file"],
        },
      },
      null,
      2,
    ),
  );
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const discovered = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(discovered);
      });
    });
  });
}
