import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-view-chat-context-"));
const port = await openPort();
const token = "rusty-view-chat-context-token";
writeRuntimeConfig(root);
const host = await startHost();

try {
  const provider = await post("/v1/admin/model-providers", token, {
    alias: "default",
    status: "active",
    protocol: "chat_completions",
    providerKind: "local",
    displayName: "Local GPT",
    baseUrl: "http://127.0.0.1:18082/v1",
    modelId: "gpt",
    contextWindowTokens: 128_000,
    maxOutputTokens: 4_096,
    temperature: 0.5,
    reasoningEffort: "low",
    reasoningFormat: "none",
  });
  assert.ok(provider.status === 200 || provider.status === 201);

  const contextUsage = await get(
    "/v1/chat/sessions/chat-session/context",
    token,
  );
  assert.equal(contextUsage.status, 200);
  assert.equal(contextUsage.body.data.session_id, "chat-session");
  assert.equal(contextUsage.body.data.provider.alias, "default");
  assert.equal(contextUsage.body.data.provider.model_id, "gpt");
  assert.equal(contextUsage.body.data.provider.temperature, 0.5);
  assert.equal(contextUsage.body.data.provider.context_window_tokens, 128_000);
  assert.equal(contextUsage.body.data.context.estimate_quality, "approximate");
  assert.equal(typeof contextUsage.body.data.brain.backend, "string");

  const commandCatalog = await get("/v1/chat/commands", token);
  assert.equal(commandCatalog.status, 200);
  assert.ok(
    commandCatalog.body.data.commands.some(
      (item: { name: string }) => item.name === "model",
    ),
  );

  const modelCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/model",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(modelCommand.status, 200);
  assert.equal(modelCommand.body.data.status, "completed");
  assert.equal(modelCommand.body.data.command_name, "model");
  assert.equal(modelCommand.body.data.response.fields.providerAlias, "default");
  assert.equal(modelCommand.body.data.response.fields.modelId, "gpt");
  assert.equal(
    modelCommand.body.data.response.fields.contextWindowTokens,
    128_000,
  );

  console.log(
    JSON.stringify(
      {
        ok: true,
        contextSession: contextUsage.body.data.session_id,
        modelProvider: modelCommand.body.data.response.fields.providerAlias,
        brainBackend: modelCommand.body.data.response.fields.brainBackend,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

async function startHost() {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "0",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "0",
    },
    bridge: await loadNativeBridge(),
  });
}

async function get(path: string, bearer?: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: bearer ? { authorization: `Bearer ${bearer}` } : undefined,
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function post(path: string, bearer: string, body: unknown) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${bearer}`,
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

function writeRuntimeConfig(dataRoot: string): void {
  const configDir = join(dataRoot, "config");
  const profilesDir = join(configDir, "profiles");
  const skillsDir = join(configDir, "skills");
  mkdirSync(profilesDir, { recursive: true });
  mkdirSync(skillsDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        skillsDir,
        brains: [{ profileId: "chat-profile" }],
        sessions: [
          {
            sessionId: "chat-session",
            agentId: "chat-agent",
            profileId: "chat-profile",
            kind: "full",
          },
        ],
        mcpBindings: [
          {
            bindingId: "chat-profile-den",
            adapterId: "mcp-ts-main",
            agentId: "chat-agent",
            sessionId: "chat-session",
            profileId: "chat-profile",
            serverNames: ["den"],
            endpointRef: "http://127.0.0.1:5199/mcp",
            transport: "streamable_http",
            toolProfileKey: "planner",
            status: "active",
            diagnostics: {},
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "chat-profile.json"),
    JSON.stringify(
      {
        profileId: "chat-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        prompt: {
          system: "Chat profile system prompt.",
          instructions: ["Answer concisely."],
        },
        localToolProfileId: "full-agent",
        toolPolicy: {
          requestedToolsets: ["session", "filesystem"],
          requestedTools: ["session_search"],
        },
      },
      null,
      2,
    ),
  );
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createTcpServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const open = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(open);
      });
    });
  });
}
