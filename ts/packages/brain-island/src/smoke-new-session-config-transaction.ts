import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

await smokeSuccessfulNewSessionConfigMove();
await smokeFailedNewSessionLeavesConfigUntouched();

console.log("new session config transaction smoke passed");

async function smokeSuccessfulNewSessionConfigMove(): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), "rusty-new-session-config-"));
  writeRuntimeConfig(root);
  const token = "new-session-config-token";
  const port = await openPort();
  const host = await startHost(root, port, token);
  let newSessionId = "";
  try {
    const response = await post(
      port,
      token,
      "/v1/admin/control/sessions/old-session/new",
      {
        reason: "transaction smoke",
        reasonCode: "slash_command_new",
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response.body));
    assert.equal(response.body.ok, true, JSON.stringify(response.body));
    assert.equal(response.body.data.outcome.status, "completed");
    newSessionId = String(
      (
        response.body.data.outcome.affectedIds as
          | { newSessionId?: unknown }
          | undefined
      )?.newSessionId,
    );
    assert.match(newSessionId, /^agent-alpha-session-/);

    const runtimeConfig = readRuntimeConfig(root);
    assert.deepEqual(
      runtimeConfig.sessions?.map((session) => session.sessionId),
      [newSessionId],
    );
    assert.equal(runtimeConfig.channelBindings?.[0]?.sessionId, newSessionId);
    assert.equal(runtimeConfig.mcpBindings?.[0]?.sessionId, newSessionId);
    assert.equal(
      runtimeConfig.scheduledJobs?.[0]?.targetSessionId,
      newSessionId,
    );

    const sessions = await host.bridge.listSessions();
    assert.equal(
      sessions.find((session) => session.sessionId === "old-session")?.status,
      "archived",
    );
    assert.equal(
      sessions.find((session) => session.sessionId === newSessionId)?.status,
      "idle",
    );
    const events = await get(port, token, "/v1/admin/events/recent?limit=40");
    assert.equal(events.status, 200, JSON.stringify(events.body));
    const eventTypes = (
      (events.body.data?.items ?? []) as Array<{
        eventType?: string;
        event_type?: string;
      }>
    ).map((event) => event.eventType ?? event.event_type);
    assert.ok(
      eventTypes.includes("new_session_brain_catalog_rebuilt"),
      "moving MCP bindings through /new should rebuild the live brain catalog",
    );
  } finally {
    await host.stop();
  }

  const restartPort = await openPort();
  const restarted = await startHost(root, restartPort, token);
  try {
    const sessions = await restarted.bridge.listSessions();
    assert.equal(
      sessions.find((session) => session.sessionId === newSessionId)?.status,
      "idle",
    );
  } finally {
    await restarted.stop();
  }
}

async function smokeFailedNewSessionLeavesConfigUntouched(): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), "rusty-new-session-config-fail-"));
  writeRuntimeConfig(root);
  const token = "new-session-config-fail-token";
  const port = await openPort();
  const host = await startHost(root, port, token);
  const configPath = join(root, "config", "service.json");
  const parsed = readRuntimeConfig(root);
  parsed.sessions?.push({
    sessionId: "duplicate-agent-session",
    agentId: "agent-alpha",
    profileId: "alpha-profile",
    kind: "full",
  });
  const invalidConfig = `${JSON.stringify(parsed, null, 2)}\n`;
  writeFileSync(configPath, invalidConfig);
  try {
    const response = await post(
      port,
      token,
      "/v1/admin/control/sessions/old-session/new",
      {
        reason: "transaction failure smoke",
        reasonCode: "slash_command_new",
      },
    );
    assert.equal(response.status, 200, JSON.stringify(response.body));
    assert.equal(response.body.ok, true, JSON.stringify(response.body));
    assert.equal(response.body.data.outcome.status, "failed");
    assert.match(
      response.body.data.outcome.summary,
      /duplicate configured agent agent-alpha/,
    );
    assert.equal(readFileSync(configPath, "utf8"), invalidConfig);

    const sessions = await host.bridge.listSessions();
    assert.notEqual(
      sessions.find((session) => session.sessionId === "old-session")?.status,
      "archived",
    );
  } finally {
    await host.stop();
  }
}

async function startHost(root: string, port: number, token: string) {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "10000",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "10000",
    },
  });
}

async function post(
  port: number,
  token: string,
  path: string,
  body: unknown,
): Promise<{ status: number; body: any }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${token}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function get(
  port: number,
  token: string,
  path: string,
): Promise<{ status: number; body: any }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: {
      authorization: `Bearer ${token}`,
    },
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
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
      const port = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(port);
      });
    });
  });
}

function readRuntimeConfig(root: string): {
  sessions?: Array<{
    sessionId?: string;
    agentId?: string;
    profileId?: string;
    kind?: "full" | "worker" | "delegated";
  }>;
  channelBindings?: Array<{ bindingId?: string; sessionId?: string }>;
  mcpBindings?: Array<{ bindingId?: string; sessionId?: string }>;
  scheduledJobs?: Array<{ id?: string; targetSessionId?: string }>;
} {
  return JSON.parse(
    readFileSync(join(root, "config", "service.json"), "utf8"),
  ) as ReturnType<typeof readRuntimeConfig>;
}

function writeRuntimeConfig(root: string): void {
  const configDir = join(root, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "alpha-profile" }],
        sessions: [
          {
            sessionId: "old-session",
            agentId: "agent-alpha",
            profileId: "alpha-profile",
            kind: "full",
          },
        ],
        channelBindings: [
          {
            bindingId: "alpha-channel",
            adapterId: "den-channel-main",
            provider: "den_channels",
            agentId: "agent-alpha",
            sessionId: "old-session",
            profileId: "alpha-profile",
            externalChannelId: "alpha-room",
            status: "active",
          },
        ],
        mcpBindings: [
          {
            bindingId: "alpha-mcp",
            adapterId: "mcp-ts",
            agentId: "agent-alpha",
            sessionId: "old-session",
            profileId: "alpha-profile",
            serverNames: ["alpha"],
            endpointRef: "config://mcp/alpha",
            transport: "stdio",
            toolProfileKey: "alpha-mcp",
            status: "active",
            diagnostics: {},
          },
        ],
        scheduledJobs: [
          {
            id: "alpha-heartbeat",
            schedule: "0 0 * * *",
            shape: "session_wake",
            targetSessionId: "old-session",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "alpha-profile.json"),
    JSON.stringify(
      {
        profileId: "alpha-profile",
        modelConfig: { provider: "local", modelName: "deterministic" },
        skills: "all",
      },
      null,
      2,
    ),
  );
}
