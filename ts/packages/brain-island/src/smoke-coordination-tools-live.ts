import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, CoreEvent } from "@rusty-crew/contracts";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

if (process.env.RUSTY_CREW_COORDINATION_LIVE !== "1") {
  console.log(
    JSON.stringify(
      {
        skipped: true,
        reason:
          "set RUSTY_CREW_COORDINATION_LIVE=1 to run the LLM-backed coordination smoke",
      },
      null,
      2,
    ),
  );
  process.exit(0);
}

const root = mkdtempSync(join(tmpdir(), "rusty-crew-coordination-live-"));
const port = await openPort();
writeRuntimeConfig(root);
const host = await startRustyCrewServiceHost({
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(port),
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "60000",
    RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "50",
    DEN_ROUTER_URL: process.env.DEN_ROUTER_URL ?? "http://127.0.0.1:18082",
  },
});

try {
  const subscription = await host.bridge.subscribeEvents({
    eventKinds: ["agent_message_routed"],
  });
  try {
    const client = createDebugApiClient({
      baseUrl: `http://127.0.0.1:${port}`,
      retries: 0,
      timeoutMs: Number.parseInt(
        process.env.RUSTY_CREW_COORDINATION_LIVE_TIMEOUT_MS ?? "120000",
        10,
      ),
    });
    const direct = await client.requestDirectDebugTurn({
      sessionId: "coordination-live-alpha-session",
      actorId: "live-smoke",
      body: "Use the send_agent_message tool exactly once. Send to toAgentId coordination-live-beta-agent with body 'COORDINATION_PING REPLY_TO=coordination-live-alpha-agent CORRELATION=live-send-proof' and correlationId live-send-proof. After the tool result, finish briefly.",
      reason: "coordination live send smoke",
    });
    assert.equal(direct.status, "accepted");
    const sendEvents = await drainUntil(
      () => host.bridge.drainSubscriptionEvents(subscription, 64),
      (events) =>
        hasMessage(events, {
          from: "coordination-live-alpha-agent",
          to: "coordination-live-beta-agent",
          correlationId: "live-send-proof",
        }) &&
        hasMessage(events, {
          from: "coordination-live-beta-agent",
          to: "coordination-live-alpha-agent",
          correlationId: "live-send-proof",
        }),
    );

    const round = await client.requestDirectDebugTurn({
      sessionId: "coordination-live-alpha-session",
      actorId: "live-smoke",
      body: "Use the agent_round tool exactly once. Send to toAgentId coordination-live-beta-agent with body 'COORDINATION_ROUND REPLY_TO=coordination-live-alpha-agent CORRELATION=live-round-proof' and correlationId live-round-proof. Wait for the reply from the tool, then finish briefly and mention the reply body.",
      reason: "coordination live round smoke",
    });
    assert.equal(round.status, "accepted");
    const roundEvents = await drainUntil(
      () => host.bridge.drainSubscriptionEvents(subscription, 64),
      (events) =>
        hasMessage(events, {
          from: "coordination-live-alpha-agent",
          to: "coordination-live-beta-agent",
          correlationId: "live-round-proof",
        }) &&
        hasMessage(events, {
          from: "coordination-live-beta-agent",
          to: "coordination-live-alpha-agent",
          correlationId: "live-round-proof",
        }),
    );

    console.log(
      JSON.stringify(
        {
          directWakeId: direct.wakeId,
          roundWakeId: round.wakeId,
          sendEvents: sendEvents.length,
          roundEvents: roundEvents.length,
          model:
            process.env.RUSTY_CREW_COORDINATION_LIVE_MODEL ?? "deepseek-flash",
        },
        null,
        2,
      ),
    );
  } finally {
    await host.bridge.unsubscribeEvents(subscription).catch(() => undefined);
  }
} finally {
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
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
            profileId: "coordination-live-alpha",
            implementationId: "coordination-live-alpha-brain",
          },
          {
            profileId: "coordination-live-beta",
            implementationId: "coordination-live-beta-brain",
          },
        ],
        sessions: [
          {
            sessionId: "coordination-live-alpha-session",
            agentId: "coordination-live-alpha-agent",
            profileId: "coordination-live-alpha",
            kind: "full",
          },
          {
            sessionId: "coordination-live-beta-session",
            agentId: "coordination-live-beta-agent",
            profileId: "coordination-live-beta",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeProfile(profilesDir, "coordination-live-alpha", [
    "You are alpha in a Rusty Crew coordination smoke.",
    "When the user asks for a tool call, call the requested tool with the exact target agent and exact correlationId.",
    "Do not invent a different target agent id or correlation id.",
  ]);
  writeProfile(profilesDir, "coordination-live-beta", [
    "You are beta in a Rusty Crew coordination smoke.",
    "When you receive an internal agent message, immediately call send_agent_message back to the sender.",
    "The inbound message body contains REPLY_TO=<agent-id> and CORRELATION=<correlation-id>; use those exact values as toAgentId and correlationId.",
    "Your reply body must start with COORDINATION_CONFIRM and include the inbound body.",
    "Do not ask questions and do not send a normal chat response instead of the tool call.",
  ]);
}

function writeProfile(
  profilesDir: string,
  profileId: string,
  instructions: string[],
): void {
  writeFileSync(
    join(profilesDir, `${profileId}.json`),
    JSON.stringify(
      {
        profileId,
        displayName: profileId,
        modelConfig: {
          provider: "den-router",
          modelName:
            process.env.RUSTY_CREW_COORDINATION_LIVE_MODEL ?? "deepseek-flash",
          baseUrl: process.env.DEN_ROUTER_URL ?? "http://127.0.0.1:18082",
          api:
            process.env.RUSTY_CREW_COORDINATION_LIVE_API ??
            "openai-completions",
          temperature: 0,
          maxOutputTokens: 256,
        },
        brain: { module: "pi-agent-core" },
        toolPolicy: { requestedToolsets: ["agent_coordination"] },
        prompt: { instructions },
      },
      null,
      2,
    ),
  );
}

async function drainUntil(
  drain: () => Promise<CoreEvent[]>,
  predicate: (events: readonly CoreEvent[]) => boolean,
): Promise<CoreEvent[]> {
  const deadline =
    Date.now() +
    Number.parseInt(
      process.env.RUSTY_CREW_COORDINATION_LIVE_TIMEOUT_MS ?? "120000",
      10,
    );
  const events: CoreEvent[] = [];
  while (Date.now() < deadline) {
    events.push(...(await drain()));
    if (predicate(events)) return events;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(
    `timed out waiting for coordination events; saw ${JSON.stringify(
      events.flatMap((event) =>
        event.type === "agent_message_routed" ? [event.message] : [],
      ),
    )}`,
  );
}

function hasMessage(
  events: readonly CoreEvent[],
  input: {
    from: string;
    to: string;
    correlationId: string;
    bodyIncludes?: string;
  },
): boolean {
  return events.some(
    (event) =>
      event.type === "agent_message_routed" &&
      event.message.from === (input.from as AgentId) &&
      event.message.to === (input.to as AgentId) &&
      event.message.correlationId === input.correlationId &&
      (input.bodyIncludes === undefined ||
        event.message.body.includes(input.bodyIncludes)),
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
