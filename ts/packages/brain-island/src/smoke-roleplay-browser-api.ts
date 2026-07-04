import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-roleplay-api-"));
const port = await openPort();
const token = "roleplay-browser-smoke-token";
writeRuntimeConfig(root);

const host = await startRustyCrewServiceHost({
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(port),
    RUSTY_CREW_ADMIN_TOKEN: token,
    RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "1000",
    RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "1000",
  },
});

try {
  const preflight = await fetch(
    `http://127.0.0.1:${port}/v1/profile/rp-profile/layers`,
    {
      method: "OPTIONS",
      headers: { origin: "http://127.0.0.1:4200" },
    },
  );
  assert.equal(preflight.status, 204);
  assert.equal(
    preflight.headers.get("access-control-allow-origin"),
    "http://127.0.0.1:4200",
  );

  const createdLayer = await post("/v1/profile/rp-profile/layers", {
    layerId: "rp-world",
    name: "RP World",
    description: "Browser smoke lore.",
    purpose: "world",
    writePolicy: "manual",
  });
  assert.equal(createdLayer.status, 200, JSON.stringify(createdLayer.body));
  assert.equal(createdLayer.body.data.layer.layer_id, "rp-world");

  const listedLayers = await get("/v1/profile/rp-profile/layers");
  assert.equal(listedLayers.status, 200);
  assert.equal(listedLayers.body.data.layers[0]?.entry_count, 0);

  const chatLayerSave = await post("/v1/admin/roleplay/lore/chat-layers", {
    chatId: "rp-session",
    layerIds: ["rp-world"],
  });
  assert.equal(chatLayerSave.status, 200, JSON.stringify(chatLayerSave.body));
  const chatLayers = await get(
    "/v1/admin/roleplay/lore/chat-layers?chat_id=rp-session",
  );
  assert.deepEqual(chatLayers.body.data.activeLayerIds, ["rp-world"]);
  const reordered = await post("/v1/admin/roleplay/lore/chat-layers/reorder", {
    chatId: "rp-session",
    layerIds: ["rp-world"],
  });
  assert.equal(reordered.status, 200);

  const character = await post(
    "/v1/admin/roleplay/profiles/rp-profile/characters",
    {
      id: "rp-hero",
      name: "RP Hero",
      description: "A browser-safe roleplay character.",
      personality: "curious and steady",
      scenario: "Smoke-testing Rusty Crew.",
      firstMessage: "Ready.",
      alternateGreetings: ["Still ready."],
      exampleMessages: ["Let's test this path."],
      tags: ["smoke"],
    },
  );
  assert.equal(character.status, 200, JSON.stringify(character.body));
  assert.equal(character.body.data.character.id, "rp-hero");
  const characters = await get(
    "/v1/admin/roleplay/profiles/rp-profile/characters",
  );
  assert.equal(characters.body.data.items.length, 1);

  const narrator = await patch(
    "/v1/admin/roleplay/profiles/rp-profile/narrator-config",
    {
      tone: "wry",
      pacing: "balanced",
      explicitness: "romantic",
      memoryDepth: "deep",
      exemplar: "Keep continuity clear.",
      review: { enabled: true, maxReviewCycles: 2 },
    },
  );
  assert.equal(narrator.status, 200, JSON.stringify(narrator.body));
  assert.equal(narrator.body.data.config.tone, "wry");
  const narratorReadback = await get(
    "/v1/admin/roleplay/profiles/rp-profile/narrator-config",
  );
  assert.equal(narratorReadback.body.data.config.memoryDepth, "deep");

  const session = await post("/v1/admin/roleplay/sessions", {
    sessionId: "rp-browser-session",
    profileId: "rp-profile",
    displayName: "Browser RP Session",
    characterId: "rp-hero",
    activeLayerIds: ["rp-world"],
  });
  assert.equal(session.status, 200, JSON.stringify(session.body));
  assert.equal(session.body.data.session.character_name, "RP Hero");
  assert.deepEqual(session.body.data.session.active_layer_ids, ["rp-world"]);

  const archived = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/archive",
    {},
  );
  assert.equal(archived.status, 200, JSON.stringify(archived.body));
  assert.equal(archived.body.data.session.archived, true);

  const restored = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/restore",
    {},
  );
  assert.equal(restored.status, 200, JSON.stringify(restored.body));
  assert.equal(restored.body.data.session.archived, false);
  assert.notEqual(restored.body.data.session.status, "archived");

  console.log(
    JSON.stringify(
      {
        loreLayer: createdLayer.body.data.layer.layer_id,
        character: character.body.data.character.id,
        session: session.body.data.session.session_id,
        narratorTone: narratorReadback.body.data.config.tone,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop();
  rmSync(root, { recursive: true, force: true });
}

async function get(path: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: { authorization: `Bearer ${token}` },
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
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

async function patch(path: string, body: unknown) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "PATCH",
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

function writeRuntimeConfig(root: string): void {
  const configDir = join(root, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "rp-profile" }],
        sessions: [
          {
            sessionId: "rp-session",
            agentId: "rp-agent",
            profileId: "rp-profile",
            kind: "full",
          },
        ],
        channelBindings: [],
        mcpBindings: [],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "rp-profile.json"),
    JSON.stringify(
      {
        profileId: "rp-profile",
        displayName: "RP Profile",
        modelConfig: { provider: "local", modelName: "deterministic" },
        brain: { module: "local" },
        toolPolicy: { requestedTools: [] },
      },
      null,
      2,
    ),
  );
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createNetServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const discoveredPort = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(discoveredPort);
      });
    });
  });
}
