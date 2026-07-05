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
    writePolicy: "auto_capture",
  });
  assert.equal(createdLayer.status, 200, JSON.stringify(createdLayer.body));
  assert.equal(createdLayer.body.data.layer.layer_id, "rp-world");
  const hiddenLayer = await post("/v1/profile/rp-profile/layers", {
    layerId: "rp-hidden",
    name: "RP Hidden",
    description: "Inactive browser smoke lore.",
    purpose: "story",
    writePolicy: "auto_capture",
  });
  assert.equal(hiddenLayer.status, 200, JSON.stringify(hiddenLayer.body));
  const manualLayer = await post("/v1/profile/rp-profile/layers", {
    layerId: "rp-manual",
    name: "RP Manual",
    description: "Durable browser-promoted lore.",
    purpose: "world",
    writePolicy: "manual",
  });
  assert.equal(manualLayer.status, 200, JSON.stringify(manualLayer.body));
  const readonlyLayer = await post("/v1/profile/rp-profile/layers", {
    layerId: "rp-readonly",
    name: "RP Readonly",
    description: "Promotion rejection target.",
    purpose: "world",
    writePolicy: "readonly",
  });
  assert.equal(readonlyLayer.status, 200, JSON.stringify(readonlyLayer.body));

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

  await captureLoreFact({
    layerId: "rp-world",
    recordId: "rp-clockmaker-song",
    worldId: "rp-world-id",
    entityId: "clockmaker",
    title: "Clockmaker Song",
    body: "The clockmaker sings three notes at dusk to wake silver leaves.",
  });
  await captureLoreFact({
    layerId: "rp-hidden",
    recordId: "rp-obsidian-vault",
    worldId: "rp-world-id",
    entityId: "vault",
    title: "Obsidian Vault",
    body: "The Obsidian Vault opens only for the forgotten royal signet.",
  });

  const promoted = await post(
    "/v1/admin/roleplay/lore/entries/rp-clockmaker-song/promote?profile_id=rp-profile",
    {
      targetLayerId: "rp-manual",
      newRecordId: "rp-clockmaker-song-promoted",
      isConstant: true,
      priority: 9,
    },
  );
  assert.equal(promoted.status, 200, JSON.stringify(promoted.body));
  assert.equal(promoted.body.data.promoted, true);
  assert.equal(
    promoted.body.data.entry.record_id,
    "rp-clockmaker-song-promoted",
  );
  assert.equal(promoted.body.data.entry.title, "Clockmaker Song");
  assert.equal(promoted.body.data.source.layerId, "rp-world");
  assert.equal(promoted.body.data.target.layerId, "rp-manual");
  assert.equal(promoted.body.data.layerEntries[0]?.layer_id, "rp-manual");
  assert.equal(promoted.body.data.layerEntries[0]?.is_constant, true);
  assert.equal(promoted.body.data.layerEntries[0]?.priority, 9);
  assert.match(
    promoted.body.data.provenance[0]?.note,
    /promoted from rp-world:rp-clockmaker-song/,
  );

  const promotedReadback = await get(
    "/v1/admin/roleplay/lore/entries/rp-clockmaker-song-promoted?layer_id=rp-manual",
  );
  assert.equal(
    promotedReadback.status,
    200,
    JSON.stringify(promotedReadback.body),
  );
  assert.equal(
    promotedReadback.body.data.entry.record_id,
    "rp-clockmaker-song-promoted",
  );
  assert.equal(
    promotedReadback.body.data.layerEntries[0]?.layer_id,
    "rp-manual",
  );

  const promotedSearch = await get(
    "/v1/admin/roleplay/lore/entries/search?q=clockmaker&layer_id=rp-manual",
  );
  assert.equal(promotedSearch.status, 200, JSON.stringify(promotedSearch.body));
  assert.deepEqual(
    promotedSearch.body.data.entries.map(
      (entry: Record<string, unknown>) => entry.record_id,
    ),
    ["rp-clockmaker-song-promoted"],
  );

  const readonlyPromotion = await post(
    "/v1/admin/roleplay/lore/entries/rp-clockmaker-song/promote",
    {
      sourceLayerId: "rp-world",
      targetLayerId: "rp-readonly",
      newRecordId: "rp-clockmaker-song-readonly",
    },
  );
  assert.equal(readonlyPromotion.status, 409);
  assert.equal(
    readonlyPromotion.body.error.reason_code,
    "roleplay_lore_target_layer_readonly",
  );

  for (let index = 0; index < 105; index += 1) {
    await captureLoreFact({
      layerId: "rp-world",
      recordId: `rp-decoy-${String(index).padStart(3, "0")}`,
      worldId: "rp-world-id",
      entityId: `decoy-${index}`,
      title: `Obsidian Decoy ${index}`,
      body: "Obsidian decoy lore that should not satisfy explicit hidden-layer filtering.",
    });
  }

  const clockmakerSearch = await get(
    "/v1/admin/roleplay/lore/entries/search?q=clockmaker&profile_id=rp-profile&limit=10&offset=0",
  );
  assert.equal(
    clockmakerSearch.status,
    200,
    JSON.stringify(clockmakerSearch.body),
  );
  assert.deepEqual(
    clockmakerSearch.body.data.entries
      .map((entry: Record<string, unknown>) => entry.record_id)
      .sort(),
    ["rp-clockmaker-song", "rp-clockmaker-song-promoted"],
  );
  assert.equal(clockmakerSearch.body.data.layerContext.source, "profile");

  const chatFilteredSearch = await get(
    "/v1/admin/roleplay/lore/entries/search?q=Vault&chat_id=rp-session",
  );
  assert.equal(chatFilteredSearch.status, 200);
  assert.equal(chatFilteredSearch.body.data.entries.length, 0);
  assert.equal(chatFilteredSearch.body.data.totalExact, true);
  assert.deepEqual(chatFilteredSearch.body.data.layerContext.activeLayerIds, [
    "rp-world",
  ]);

  const unscopedPagedSearch = await get(
    "/v1/admin/roleplay/lore/entries/search?q=Obsidian&limit=5&offset=0",
  );
  assert.equal(unscopedPagedSearch.status, 200);
  assert.equal(unscopedPagedSearch.body.data.entries.length, 5);
  assert.equal(unscopedPagedSearch.body.data.hasMore, true);
  assert.equal(unscopedPagedSearch.body.data.total, 6);
  assert.equal(unscopedPagedSearch.body.data.totalExact, false);

  const explicitLayerSearch = await get(
    "/v1/admin/roleplay/lore/entries/search?q=Obsidian&layer_id=rp-hidden",
  );
  assert.equal(explicitLayerSearch.status, 200);
  assert.equal(explicitLayerSearch.body.data.entries.length, 1);
  assert.equal(
    explicitLayerSearch.body.data.entries[0]?.record_id,
    "rp-obsidian-vault",
  );
  assert.equal(explicitLayerSearch.body.data.total, 1);
  assert.equal(explicitLayerSearch.body.data.totalExact, true);

  const createdEntry = await post("/v1/admin/roleplay/lore/entries", {
    layer_id: "rp-world",
    is_constant: true,
    priority: 7,
    write: {
      record_id: "rp-manual-entry",
      world_id: "rp-world-id",
      entity_id: "manual-entry",
      shape: { shape_id: "lore_entry", version: 1 },
      canon_status: "draft",
      visibility: "public",
      title: "Manual Entry",
      body: "The manual entry starts as a browser-created draft.",
      content: {
        world_id: "rp-world-id",
        entity_id: "manual-entry",
        title: "Manual Entry",
        body: "The manual entry starts as a browser-created draft.",
        canon_status: "draft",
        visibility: "public",
        metadata_json: { tags: ["editor-smoke"] },
      },
      evidence_refs: [
        {
          evidence_type: "ui",
          ref_id: "roleplay-browser-editor",
          label: "roleplay browser editor",
        },
      ],
      source: "ui",
      confidence: 0.9,
      durability_rationale: "Created by browser API editor smoke.",
    },
  });
  assert.equal(createdEntry.status, 200, JSON.stringify(createdEntry.body));
  assert.equal(createdEntry.body.data.entry.record_id, "rp-manual-entry");
  assert.equal(createdEntry.body.data.entry.revision, 1);
  assert.equal(createdEntry.body.data.layerEntries.length, 1);
  assert.equal(createdEntry.body.data.layerEntries[0]?.layer_id, "rp-world");
  assert.equal(createdEntry.body.data.provenance.length, 1);

  const readEntry = await get(
    "/v1/admin/roleplay/lore/entries/rp-manual-entry?layer_id=rp-world",
  );
  assert.equal(readEntry.status, 200, JSON.stringify(readEntry.body));
  assert.equal(readEntry.body.data.entry.title, "Manual Entry");
  assert.equal(
    readEntry.body.data.layerEntries[0]?.record_id,
    "rp-manual-entry",
  );
  assert.equal(readEntry.body.data.provenance[0]?.record_id, "rp-manual-entry");

  const patchedEntry = await patch(
    "/v1/admin/roleplay/lore/entries/rp-manual-entry?layer_id=rp-world",
    {
      expected_revision: readEntry.body.data.entry.revision,
      title: "Manual Entry Revised",
      body: "The manual entry was revised by the browser editor.",
      canon_status: "canon",
      content: {
        metadata_json: { tags: ["editor-smoke", "revised"] },
      },
      evidence_refs: [
        {
          evidence_type: "ui",
          ref_id: "roleplay-browser-editor-revision",
          label: "roleplay browser editor revision",
        },
      ],
      confidence: 0.95,
      durability_rationale: "Updated by browser API editor smoke.",
    },
  );
  assert.equal(patchedEntry.status, 200, JSON.stringify(patchedEntry.body));
  assert.equal(patchedEntry.body.data.entry.title, "Manual Entry Revised");
  assert.equal(patchedEntry.body.data.entry.canon_status, "canon");
  assert.equal(patchedEntry.body.data.entry.revision, 2);
  assert.equal(patchedEntry.body.data.provenance.length, 2);
  assert.equal(
    patchedEntry.body.data.entry.content.metadata_json.tags[1],
    "revised",
  );

  const layerEntryReadback = await get(
    "/v1/admin/roleplay/lore/layers/rp-world/entries",
  );
  const manualJoin = layerEntryReadback.body.data.entries.find(
    (entry: Record<string, unknown>) => entry.record_id === "rp-manual-entry",
  );
  assert.equal(manualJoin?.layer_id, "rp-world");
  assert.equal(
    (manualJoin?.record as Record<string, unknown> | undefined)?.title,
    "Manual Entry Revised",
  );

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
      stylePrompt:
        "Use lean, emotionally direct narration with one vivid sensory anchor.",
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
  assert.equal(
    narratorReadback.body.data.config.stylePrompt,
    "Use lean, emotionally direct narration with one vivid sensory anchor.",
  );
  assert.equal(
    narratorReadback.body.data.config.exemplar,
    "Keep continuity clear.",
  );

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

async function captureLoreFact(input: {
  layerId: string;
  recordId: string;
  worldId: string;
  entityId: string;
  title: string;
  body: string;
}) {
  const response = await post("/v1/admin/roleplay/lore/facts/capture", {
    layer_id: input.layerId,
    write: {
      record_id: input.recordId,
      world_id: input.worldId,
      entity_id: input.entityId,
      shape: {
        shape_id: "lore_entry",
        version: 1,
      },
      canon_status: "canon",
      visibility: "public",
      title: input.title,
      body: input.body,
      content: {
        world_id: input.worldId,
        entity_id: input.entityId,
        title: input.title,
        body: input.body,
        canon_status: "canon",
        visibility: "public",
      },
      evidence_refs: [
        {
          evidence_type: "import",
          ref_id: "roleplay-browser-smoke",
          label: "roleplay browser smoke",
        },
      ],
      source: "import",
      confidence: 1,
      durability_rationale: "Seed fixture for roleplay browser API smoke.",
      now: new Date("2026-07-05T00:00:00.000Z").toISOString(),
    },
    is_constant: false,
    priority: 1,
    capture_reason: "roleplay-browser-api-smoke",
  });
  assert.equal(response.status, 200, JSON.stringify(response.body));
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
