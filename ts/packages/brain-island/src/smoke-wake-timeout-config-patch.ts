import assert from "node:assert/strict";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-wake-timeout-patch-"));
const port = await openPort();

try {
  writeRuntimeFixture(root);
  const host = await startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "60000",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "60000",
    },
  });
  try {
    const capabilities = await getJson("/v1/admin/capabilities");
    assert.equal(capabilities.status, 200);
    assert.equal(capabilities.body.ok, true);
    assert.ok(
      capabilities.body.data.capabilities.some(
        (capability: { id?: string; path_template?: string }) =>
          capability.id === "admin.control.config.wake_timeout.patch" &&
          capability.path_template === "/v1/admin/control/config/wake-timeout",
      ),
      "capability registry should advertise the safe wake-timeout patch path",
    );

    const before = runtimeConfigSnapshot(root);
    assert.deepEqual(before.sections, {
      brains: 1,
      sessions: 1,
      scheduledJobs: 1,
      channelBindings: 1,
      mcpServers: 1,
      mcpBindings: 1,
    });

    const disabled = await postJson("/v1/admin/control/config/wake-timeout", {
      wakeTimeout: { mode: "disabled" },
    });
    assert.equal(disabled.status, 200, JSON.stringify(disabled.body));
    assert.equal(disabled.body.ok, true);
    assert.deepEqual(disabled.body.data.outcome.result.wakeTimeout, {
      mode: "disabled",
    });
    assert.equal(
      disabled.body.data.outcome.result.safeWritePath.capabilityId,
      "admin.control.config.wake_timeout.patch",
    );
    const afterDisabled = runtimeConfigSnapshot(root);
    assert.deepEqual(afterDisabled.sections, before.sections);
    assert.deepEqual(afterDisabled.wakeTimeout, { mode: "disabled" });

    const defaulted = await postJson("/v1/admin/control/config/wake-timeout", {
      wakeTimeout: { mode: "default", defaultMs: 60_000 },
    });
    assert.equal(defaulted.status, 200, JSON.stringify(defaulted.body));
    assert.equal(defaulted.body.ok, true);
    assert.deepEqual(defaulted.body.data.outcome.result.wakeTimeout, {
      mode: "default",
      defaultMs: 60_000,
    });
    const afterDefault = runtimeConfigSnapshot(root);
    assert.deepEqual(afterDefault.sections, before.sections);
    assert.deepEqual(afterDefault.wakeTimeout, {
      mode: "default",
      defaultMs: 60_000,
    });

    console.log(
      JSON.stringify(
        {
          capability: "admin.control.config.wake_timeout.patch",
          preservedSections: afterDefault.sections,
          wakeTimeout: afterDefault.wakeTimeout,
        },
        null,
        2,
      ),
    );
  } finally {
    await host.stop();
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}

function writeRuntimeFixture(rootDir: string): void {
  const configDir = join(rootDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        wakeTimeout: { mode: "default", defaultMs: 30_000 },
        brains: [{ profileId: "patch-profile" }],
        sessions: [
          {
            sessionId: "patch-session",
            agentId: "patch-agent",
            profileId: "patch-profile",
            kind: "full",
          },
        ],
        scheduledJobs: [
          {
            id: "patch-diagnostics",
            schedule: "*/15 * * * *",
            shape: "host_job",
            jobKind: "runtime.diagnostics.snapshot",
            payload: { schema_version: 1 },
          },
        ],
        channelBindings: [
          {
            bindingId: "patch-channel",
            adapterId: "den-channel-main",
            provider: "den_channels",
            agentId: "patch-agent",
            sessionId: "patch-session",
            profileId: "patch-profile",
            externalChannelId: "patch-room",
            status: "disconnected",
          },
        ],
        mcpServers: [
          {
            id: "patch-mcp",
            label: "Patch MCP",
            baseUrl: "http://127.0.0.1:9/mcp",
            transport: "streamable_http",
          },
        ],
        mcpBindings: [
          {
            bindingId: "patch-mcp-binding",
            adapterId: "mcp-ts",
            agentId: "patch-agent",
            sessionId: "patch-session",
            profileId: "patch-profile",
            serverNames: ["patch-mcp"],
            endpointRef: "config://mcp/patch-mcp",
            transport: "streamable_http",
            toolProfileKey: "patch-profile-mcp",
            status: "disconnected",
            diagnostics: {},
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "patch-profile.json"),
    JSON.stringify(
      {
        profileId: "patch-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        toolPolicy: {
          requestedTools: [],
        },
      },
      null,
      2,
    ),
  );
}

function runtimeConfigSnapshot(rootDir: string): {
  wakeTimeout?: unknown;
  sections: Record<string, number | undefined>;
} {
  const value = JSON.parse(
    readFileSync(join(rootDir, "config", "service.json"), "utf8"),
  ) as Record<string, unknown>;
  return {
    wakeTimeout: value.wakeTimeout,
    sections: {
      brains: arraySectionLength(value.brains),
      sessions: arraySectionLength(value.sessions),
      scheduledJobs: arraySectionLength(value.scheduledJobs),
      channelBindings: arraySectionLength(value.channelBindings),
      mcpServers: arraySectionLength(value.mcpServers),
      mcpBindings: arraySectionLength(value.mcpBindings),
    },
  };
}

function arraySectionLength(value: unknown): number | undefined {
  return Array.isArray(value) ? value.length : undefined;
}

async function getJson(path: string): Promise<{ status: number; body: any }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`);
  return { status: response.status, body: await response.json() };
}

async function postJson(
  path: string,
  body: unknown,
): Promise<{ status: number; body: any }> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: await response.json() };
}

async function openPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  if (!address || typeof address === "string") {
    throw new Error("failed to allocate port");
  }
  return address.port;
}
