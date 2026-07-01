import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, SessionId } from "@rusty-crew/contracts";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-rebuild-replacement-"));
const port = await openPort();
const token = "runtime-rebuild-replacement-token";
writeRuntimeConfig(root);

const host = await startRustyCrewServiceHost({
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

try {
  await host.bridge.createProfileRegistryRecord({
    profileId: "replace-profile",
    lifecycleStatus: "active",
    defaultSessionKind: "full",
    agentId: "replace-agent",
    activeRuntimeSettingsJson: {},
    sourceAssetRefs: [],
    derivedRuntimeRefs: [
      {
        refKind: "brain",
        refId: "replace-profile-brain",
        status: "planned",
        metadataJson: { profile_id: "replace-profile" },
      },
      {
        refKind: "session",
        refId: "old-session",
        status: "planned",
        metadataJson: {
          agent_id: "replace-agent",
          profile_id: "replace-profile",
          session_id: "old-session",
        },
      },
    ],
    importExport: { metadataJson: {} },
    now: new Date().toISOString(),
  });

  await host.bridge.enqueueBodyFollowUpMessage({
    sessionId: "old-session" as SessionId,
    from: "operator" as AgentId,
    body: "This pending message must remain on the archived old session.",
  });
  const defaultPlan = await post(
    "/v1/admin/control/sessions/old-session/rebuild-runtime/plan",
    {
      reason: "replacement smoke default channel behavior",
      sessionIdentity: "replace",
      newSessionId: "new-session",
    },
  );
  assert.equal(defaultPlan.status, 200, JSON.stringify(defaultPlan.body));
  assert.equal(
    (
      defaultPlan.body.data.outcome.result as {
        channelBindings?: { action?: string };
      }
    ).channelBindings?.action,
    "unchanged",
  );

  const replacement = await post(
    "/v1/admin/control/sessions/old-session/rebuild-runtime/apply",
    {
      reason: "replacement smoke",
      sessionIdentity: "replace",
      newSessionId: "new-session",
      channelBindingAction: "move",
    },
  );
  assert.equal(replacement.status, 200, JSON.stringify(replacement.body));
  assert.equal(replacement.body.ok, true);
  assert.equal(replacement.body.data.outcome.status, "completed");
  const result = replacement.body.data.outcome.result as {
    preservesSessionId?: boolean;
    preservesHistory?: boolean;
    channelBindings?: { action?: string; bindingIds?: string[] };
    mcp?: {
      bindingIds?: string[];
      refreshedBindingIds?: string[];
      degradedBindingIds?: string[];
    };
    queuedMessages?: { action?: string };
    profileRegistry?: {
      action?: string;
      updatedProfileId?: string;
      updatedRefIds?: string[];
    };
    apply?: {
      replacementSession?: {
        oldSessionId?: string;
        newSessionId?: string;
        queuedMessages?: { expiredQueuedMessagesCopied?: boolean };
      };
    };
  };
  assert.equal(result.preservesSessionId, false);
  assert.equal(result.preservesHistory, false);
  assert.equal(result.channelBindings?.action, "move_to_replacement_session");
  assert.deepEqual(result.channelBindings?.bindingIds, ["replace-channel"]);
  assert.deepEqual(result.mcp?.bindingIds, ["replace-mcp"]);
  assert.equal(
    [
      ...(result.mcp?.refreshedBindingIds ?? []),
      ...(result.mcp?.degradedBindingIds ?? []),
    ].includes("replace-mcp"),
    true,
  );
  assert.equal(
    result.queuedMessages?.action,
    "start_replacement_session_with_empty_queue",
  );
  assert.equal(result.profileRegistry?.action, "update_session_refs");
  assert.equal(result.profileRegistry?.updatedProfileId, "replace-profile");
  assert.deepEqual(result.profileRegistry?.updatedRefIds, ["old-session"]);
  assert.equal(result.apply?.replacementSession?.oldSessionId, "old-session");
  assert.equal(result.apply?.replacementSession?.newSessionId, "new-session");
  assert.equal(
    result.apply?.replacementSession?.queuedMessages
      ?.expiredQueuedMessagesCopied,
    false,
  );

  const sessions = await host.bridge.listSessions();
  assert.equal(
    sessions.find((session) => session.sessionId === "old-session")?.status,
    "archived",
  );
  assert.equal(
    sessions.find((session) => session.sessionId === "new-session")?.status,
    "idle",
  );
  const replacementBodyState = JSON.parse(
    new TextDecoder().decode(
      await host.bridge.projectBodyStateJson("new-session"),
    ),
  ) as { pendingMessages?: unknown[] };
  assert.equal(replacementBodyState.pendingMessages?.length ?? 0, 0);

  const runtimeConfig = JSON.parse(
    readFileSync(join(root, "config", "service.json"), "utf8"),
  ) as {
    sessions?: Array<{ sessionId?: string }>;
    channelBindings?: Array<{ bindingId?: string; sessionId?: string }>;
    mcpBindings?: Array<{ bindingId?: string; sessionId?: string }>;
    scheduledJobs?: Array<{ id?: string; targetSessionId?: string }>;
  };
  assert.equal(runtimeConfig.sessions?.[0]?.sessionId, "new-session");
  assert.equal(runtimeConfig.channelBindings?.[0]?.sessionId, "new-session");
  assert.equal(runtimeConfig.mcpBindings?.[0]?.sessionId, "new-session");
  assert.equal(
    runtimeConfig.scheduledJobs?.[0]?.targetSessionId,
    "new-session",
  );
  const registryRecord =
    await host.bridge.getProfileRegistryRecord("replace-profile");
  const sessionRef = registryRecord?.derivedRuntimeRefs.find(
    (ref) => ref.refKind === "session",
  );
  assert.equal(sessionRef?.refId, "new-session");
  assert.equal(
    (sessionRef?.metadataJson as { session_id?: string } | undefined)
      ?.session_id,
    "new-session",
  );
  console.log("runtime rebuild replacement smoke passed");
} finally {
  await host.stop();
}

async function post(path: string, body: unknown) {
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

function writeRuntimeConfig(root: string): void {
  const configDir = join(root, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "replace-profile" }],
        sessions: [
          {
            sessionId: "old-session",
            agentId: "replace-agent",
            profileId: "replace-profile",
            kind: "full",
          },
        ],
        channelBindings: [
          {
            bindingId: "replace-channel",
            adapterId: "den-channel-main",
            provider: "den_channels",
            agentId: "replace-agent",
            sessionId: "old-session",
            profileId: "replace-profile",
            externalChannelId: "replace-room",
            status: "active",
          },
        ],
        mcpBindings: [
          {
            bindingId: "replace-mcp",
            adapterId: "mcp-ts",
            agentId: "replace-agent",
            sessionId: "old-session",
            profileId: "replace-profile",
            serverNames: ["replace"],
            endpointRef: "config://mcp/replace",
            transport: "stdio",
            toolProfileKey: "replace-mcp",
            status: "active",
            diagnostics: {},
          },
        ],
        scheduledJobs: [
          {
            id: "replace-heartbeat",
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
    join(profilesDir, "replace-profile.json"),
    JSON.stringify(
      {
        profileId: "replace-profile",
        modelConfig: { provider: "local", modelName: "deterministic" },
        skills: "all",
      },
      null,
      2,
    ),
  );
}
