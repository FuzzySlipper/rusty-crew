import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-roleplay-api-"));
const port = await openPort();
const token = "roleplay-browser-smoke-token";
writeRuntimeConfig(root);
const bridge = await loadNativeBridge();

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
  bridge,
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
  assert.equal(
    promoted.body.data.supersession.supersedesRecordId,
    "rp-clockmaker-song",
  );
  assert.equal(
    promoted.body.data.supersession.supersedes.record_id,
    "rp-clockmaker-song",
  );
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
    promotedReadback.body.data.supersession.supersedesRecordId,
    "rp-clockmaker-song",
  );
  assert.equal(
    promotedReadback.body.data.layerEntries[0]?.layer_id,
    "rp-manual",
  );

  const promotedSourceReadback = await get(
    "/v1/admin/roleplay/lore/entries/rp-clockmaker-song?layer_id=rp-world",
  );
  assert.equal(
    promotedSourceReadback.status,
    200,
    JSON.stringify(promotedSourceReadback.body),
  );
  assert.equal(
    promotedSourceReadback.body.data.supersession.supersededByRecordId,
    "rp-clockmaker-song-promoted",
  );
  assert.equal(
    promotedSourceReadback.body.data.supersession.supersededBy.record_id,
    "rp-clockmaker-song-promoted",
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
    ["rp-clockmaker-song-promoted"],
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
        lore_controls: {
          primary_keys: ["manual key"],
          secondary_keys: ["secondary smoke"],
          enabled: true,
          constant: true,
          scan_depth: 8,
          insertion_position: "lore_block",
          insertion_order: 7,
          probability: 0.75,
          retrieval_role: "system",
        },
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
  assert.deepEqual(createdEntry.body.data.entry.primary_keys, ["manual key"]);
  assert.equal(createdEntry.body.data.entry.lore_controls.scan_depth, 8);
  assert.equal(createdEntry.body.data.entry.lore_controls.probability, 0.75);
  assert.equal(
    createdEntry.body.data.entry.lore_control_support.primary_keys,
    "stored_only",
  );
  assert.equal(createdEntry.body.data.layerEntries.length, 1);
  assert.equal(createdEntry.body.data.layerEntries[0]?.layer_id, "rp-world");
  assert.equal(createdEntry.body.data.layerEntries[0]?.constant, true);
  assert.equal(createdEntry.body.data.layerEntries[0]?.insertion_order, 7);
  assert.equal(createdEntry.body.data.provenance.length, 1);

  const readEntry = await get(
    "/v1/admin/roleplay/lore/entries/rp-manual-entry?layer_id=rp-world",
  );
  assert.equal(readEntry.status, 200, JSON.stringify(readEntry.body));
  assert.equal(readEntry.body.data.entry.title, "Manual Entry");
  assert.deepEqual(readEntry.body.data.entry.secondary_keys, [
    "secondary smoke",
  ]);
  assert.equal(
    readEntry.body.data.layerEntries[0]?.record_id,
    "rp-manual-entry",
  );
  assert.equal(
    readEntry.body.data.layerEntries[0]?.lore_controls.constant,
    true,
  );
  assert.equal(readEntry.body.data.provenance[0]?.record_id, "rp-manual-entry");

  const layerEntryPatch = await patch(
    "/v1/admin/roleplay/lore/layers/rp-world/entries/rp-manual-entry",
    {
      is_constant: false,
      priority: 3,
    },
  );
  assert.equal(
    layerEntryPatch.status,
    200,
    JSON.stringify(layerEntryPatch.body),
  );
  assert.equal(layerEntryPatch.body.data.layerEntry.constant, false);
  assert.equal(layerEntryPatch.body.data.layerEntry.insertion_order, 3);

  const layerEntryRestore = await patch(
    "/v1/admin/roleplay/lore/layers/rp-world/entries/rp-manual-entry",
    {
      constant: true,
      insertion_order: 2,
    },
  );
  assert.equal(
    layerEntryRestore.status,
    200,
    JSON.stringify(layerEntryRestore.body),
  );
  assert.equal(layerEntryRestore.body.data.layerEntry.constant, true);
  assert.equal(layerEntryRestore.body.data.layerEntry.insertion_order, 2);

  const recall = await bridge.recallLore({
    chat_id: "rp-session",
    session_id: "rp-session",
    query_text: "unrelated query should still include constants",
    active_subjects: [],
    excluded_subjects: [],
    token_budget: 10_000,
    trace_id: "rp-browser-trigger-controls",
    record_trace: true,
    now: new Date("2026-07-05T00:00:00.000Z").toISOString(),
  });
  assert.ok(
    (recall.entries as Array<Record<string, unknown>>).some(
      (entry) =>
        (entry.record as Record<string, unknown>).record_id ===
          "rp-manual-entry" && entry.is_constant === true,
    ),
  );

  const patchedEntry = await patch(
    "/v1/admin/roleplay/lore/entries/rp-manual-entry?layer_id=rp-world",
    {
      expected_revision: readEntry.body.data.entry.revision,
      title: "Manual Entry Revised",
      body: "The manual entry was revised by the browser editor.",
      canon_status: "canon",
      primary_keys: ["revised primary"],
      secondary_keys: ["revised secondary"],
      enabled: true,
      scan_depth: 5,
      insertion_position: "after_history",
      insertion_order: 2,
      probability: 1,
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
  assert.deepEqual(patchedEntry.body.data.entry.primary_keys, [
    "revised primary",
  ]);
  assert.equal(
    patchedEntry.body.data.entry.lore_controls.insertion_position,
    "after_history",
  );
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
  assert.equal(manualJoin?.constant, true);
  assert.equal(manualJoin?.insertion_order, 2);
  assert.equal(
    (manualJoin?.record as Record<string, unknown> | undefined)?.title,
    "Manual Entry Revised",
  );
  assert.deepEqual(
    (
      (manualJoin?.record as Record<string, unknown> | undefined)
        ?.lore_controls as Record<string, unknown> | undefined
    )?.primary_keys,
    ["revised primary"],
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
      avatarUrl: "https://example.invalid/hero.png",
    },
  );
  assert.equal(character.status, 200, JSON.stringify(character.body));
  assert.equal(character.body.data.character.id, "rp-hero");
  const characters = await get(
    "/v1/admin/roleplay/profiles/rp-profile/characters",
  );
  assert.equal(characters.body.data.items.length, 1);

  const persona = await post(
    "/v1/admin/roleplay/profiles/rp-profile/personas",
    {
      id: "rp-player",
      displayName: "Browser Player",
      avatarUrl: "https://example.invalid/player.png",
      avatarAssetRef: "asset://player/browser",
      description: "A player-side speaker with persistent identity.",
      notes: "Prefers curious first-person choices.",
    },
  );
  assert.equal(persona.status, 200, JSON.stringify(persona.body));
  assert.equal(persona.body.data.persona.id, "rp-player");
  assert.equal(persona.body.data.persona.profileId, "rp-profile");
  assert.equal(persona.body.data.persona.status, "active");
  const personas = await get("/v1/admin/roleplay/profiles/rp-profile/personas");
  assert.equal(personas.status, 200, JSON.stringify(personas.body));
  assert.equal(personas.body.data.items.length, 1);
  assert.equal(personas.body.data.items[0]?.displayName, "Browser Player");
  const patchedPersona = await patch(
    "/v1/admin/roleplay/profiles/rp-profile/personas/rp-player",
    {
      displayName: "Browser Player Revised",
      notes: "Persona edits should not affect old transcript snapshots later.",
    },
  );
  assert.equal(patchedPersona.status, 200, JSON.stringify(patchedPersona.body));
  assert.equal(
    patchedPersona.body.data.persona.displayName,
    "Browser Player Revised",
  );

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
    playerPersonaId: "rp-player",
    characterId: "rp-hero",
    activeLayerIds: ["rp-world"],
  });
  assert.equal(session.status, 200, JSON.stringify(session.body));
  assert.equal(session.body.data.session.player_persona_id, "rp-player");
  assert.equal(
    session.body.data.session.player_persona_display_name,
    "Browser Player Revised",
  );
  assert.equal(session.body.data.session.player_persona_source, "persona");
  assert.equal(session.body.data.session.character_name, "RP Hero");
  assert.deepEqual(session.body.data.session.active_layer_ids, ["rp-world"]);

  const userSlot = await post("/v1/chat/sessions/rp-browser-session/slots", {
    slot_id: "rp-user-slot",
    primary_variant_id: "rp-user-primary",
    message_id: "rp-message-user-1",
    actor: { id: "rp-player", kind: "human" },
    body: "I lift the lantern.",
  });
  assert.equal(userSlot.status, 201, JSON.stringify(userSlot.body));
  assert.equal(
    userSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .speaker_kind,
    "player_persona",
  );
  assert.equal(
    userSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .display_name,
    "Browser Player Revised",
  );
  assert.equal(
    userSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .avatar_asset_ref,
    "asset://player/browser",
  );
  const assistantSlot = await post(
    "/v1/chat/sessions/rp-browser-session/slots",
    {
      slot_id: "rp-assistant-slot",
      primary_variant_id: "rp-assistant-primary",
      message_id: "rp-message-assistant-1",
      actor: { id: "rp-profile", kind: "agent" },
      body: "The lantern catches silver dust in the air.",
    },
  );
  assert.equal(assistantSlot.status, 201, JSON.stringify(assistantSlot.body));
  assert.equal(
    assistantSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .speaker_kind,
    "assistant_character",
  );
  assert.equal(
    assistantSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .display_name,
    "RP Hero",
  );
  assert.equal(
    assistantSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .avatar_url,
    "https://example.invalid/hero.png",
  );

  const switchedPersona = await patch(
    "/v1/admin/roleplay/profiles/rp-profile/personas/rp-player",
    { displayName: "Browser Player Later" },
  );
  assert.equal(switchedPersona.status, 200);
  const switchedCharacter = await patch(
    "/v1/admin/roleplay/profiles/rp-profile/characters/rp-hero",
    { name: "RP Hero Later" },
  );
  assert.equal(switchedCharacter.status, 200);
  const slotReadback = await get(
    "/v1/chat/sessions/rp-browser-session/slots?include_alternates=true",
  );
  const originalUserSlot = slotReadback.body.data.items.find(
    (slot: Record<string, unknown>) => slot.slot_id === "rp-user-slot",
  );
  assert.equal(
    (originalUserSlot?.primary as any).message.metadata_json.speaker_identity
      .display_name,
    "Browser Player Revised",
  );

  const initialAlternatives = await get(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
  );
  assert.equal(initialAlternatives.status, 200);
  assert.equal(initialAlternatives.body.data.slot.slot_id, "rp-assistant-slot");
  assert.equal(initialAlternatives.body.data.slot.alternate_count, 0);

  const firstAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
    {
      slotId: "rp-assistant-slot",
      variantId: "rp-alt-1",
      messageId: "rp-message-assistant-alt-1",
      body: "The lantern shows a narrow blue seam under the old door.",
    },
  );
  assert.equal(
    firstAlternative.status,
    201,
    JSON.stringify(firstAlternative.body),
  );
  const secondAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
    {
      slotId: "rp-assistant-slot",
      variantId: "rp-alt-2",
      messageId: "rp-message-assistant-alt-2",
      body: "The lantern reveals a serpent-and-rose crest in the dust.",
    },
  );
  assert.equal(
    secondAlternative.body.data.slot.alternate_count,
    2,
    JSON.stringify(secondAlternative.body),
  );
  assert.equal(
    secondAlternative.body.data.variant.message.metadata_json.speaker_identity
      .display_name,
    "RP Hero Later",
  );
  const selectedAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives/rp-assistant-slot/select",
    { variantId: "rp-alt-2" },
  );
  assert.equal(
    selectedAlternative.status,
    200,
    JSON.stringify(selectedAlternative.body),
  );
  assert.equal(
    selectedAlternative.body.data.slot.active_variant_id,
    "rp-alt-2",
  );

  const generatedSession = await post("/v1/admin/roleplay/sessions", {
    sessionId: "rp-generated-session",
    profileId: "rp-profile",
    displayName: "Generated Alternative Session",
    playerPersonaId: "rp-player",
    characterId: "rp-hero",
    activeLayerIds: ["rp-world"],
  });
  assert.equal(
    generatedSession.status,
    200,
    JSON.stringify(generatedSession.body),
  );
  const generatedUserSlot = await post(
    "/v1/chat/sessions/rp-generated-session/slots",
    {
      slot_id: "rp-generated-user-slot",
      primary_variant_id: "rp-generated-user-primary",
      message_id: "rp-message-generated-user-1",
      actor: { id: "rp-player", kind: "human" },
      body: "I press my palm to the sealed door.",
    },
  );
  assert.equal(
    generatedUserSlot.status,
    201,
    JSON.stringify(generatedUserSlot.body),
  );
  const generatedAssistantSlot = await post(
    "/v1/chat/sessions/rp-generated-session/slots",
    {
      slot_id: "rp-generated-assistant-slot",
      primary_variant_id: "rp-generated-assistant-primary",
      message_id: "rp-message-generated-assistant-1",
      actor: { id: "rp-profile", kind: "agent" },
      body: "The old seal warms beneath your hand.",
    },
  );
  assert.equal(
    generatedAssistantSlot.status,
    201,
    JSON.stringify(generatedAssistantSlot.body),
  );
  const generatedAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-generated-session/alternatives/generate",
    {
      slotId: "rp-generated-assistant-slot",
      variantId: "rp-generated-alt-1",
      messageId: "rp-message-generated-alt-1",
      instructions: "Keep it concise for the smoke test.",
    },
  );
  assert.equal(
    generatedAlternative.status,
    201,
    JSON.stringify(generatedAlternative.body),
  );
  assert.equal(generatedAlternative.body.data.status, "generated");
  assert.equal(
    generatedAlternative.body.data.slot.active_variant_id,
    "rp-generated-alt-1",
  );
  assert.equal(
    generatedAlternative.body.data.variant.message.metadata_json.generated,
    true,
  );
  const generatedWakeId =
    generatedAlternative.body.data.variant.message.metadata_json.wake_id;
  assert.equal(typeof generatedWakeId, "string");
  assert.ok(
    generatedAlternative.body.data.variant.message.body.length > 0,
    "generated alternative should persist a non-empty assistant body",
  );
  const generatedChatEvents = await get(
    "/v1/chat/sessions/rp-generated-session/events?limit=200",
  );
  assert.equal(
    generatedChatEvents.status,
    200,
    JSON.stringify(generatedChatEvents.body),
  );
  const leakedGeneratedWakeEvents = generatedChatEvents.body.data.items.filter(
    (event: { kind: string; payload?: Record<string, unknown> }) =>
      event.payload?.wake_id === generatedWakeId &&
      [
        "assistant_turn_started",
        "assistant_text_delta",
        "assistant_reasoning_delta",
        "assistant_turn_finished",
        "assistant_message_completed",
      ].includes(event.kind),
  );
  assert.deepEqual(
    leakedGeneratedWakeEvents,
    [],
    "generated alternatives must not leak a normal assistant wake turn into chat events",
  );
  const selectedGeneratedAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-generated-session/alternatives/rp-generated-assistant-slot/select",
    { variantId: "rp-generated-alt-1" },
  );
  assert.equal(
    selectedGeneratedAlternative.status,
    200,
    JSON.stringify(selectedGeneratedAlternative.body),
  );
  const generatedFollowupUserSlot = await post(
    "/v1/chat/sessions/rp-generated-session/slots",
    {
      slot_id: "rp-generated-user-slot-later",
      primary_variant_id: "rp-generated-user-primary-later",
      message_id: "rp-message-generated-user-later",
      actor: { id: "rp-player", kind: "human" },
      body: "I ask what the seal wants.",
    },
  );
  assert.equal(
    generatedFollowupUserSlot.status,
    201,
    JSON.stringify(generatedFollowupUserSlot.body),
  );
  assert.equal(
    generatedFollowupUserSlot.body.data.slot.primary.message
      .previous_message_id,
    "rp-message-generated-alt-1",
  );
  const generatedTerminalUserRejection = await post(
    "/v1/admin/roleplay/sessions/rp-generated-session/alternatives/generate",
    {},
  );
  assert.equal(generatedTerminalUserRejection.status, 400);
  assert.match(
    generatedTerminalUserRejection.body.error.message,
    /terminal message is user/,
  );

  const laterUserSlot = await post(
    "/v1/chat/sessions/rp-browser-session/slots",
    {
      slot_id: "rp-user-slot-later",
      primary_variant_id: "rp-user-primary-later",
      message_id: "rp-message-user-later",
      actor: { id: "rp-player", kind: "human" },
      body: "I lower the lantern.",
    },
  );
  assert.equal(
    laterUserSlot.body.data.slot.primary.message.metadata_json.speaker_identity
      .display_name,
    "Browser Player Later",
  );
  const staleImplicitAlternatives = await get(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
  );
  assert.equal(staleImplicitAlternatives.status, 400);
  assert.match(
    staleImplicitAlternatives.body.error.message,
    /terminal message is user/,
  );
  const staleExplicitAssistantAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
    {
      slotId: "rp-assistant-slot",
      variantId: "rp-alt-stale",
      body: "This stale alternative should be rejected.",
    },
  );
  assert.equal(staleExplicitAssistantAlternative.status, 400);
  assert.match(
    staleExplicitAssistantAlternative.body.error.message,
    /not the current terminal assistant slot/,
  );
  const staleExplicitUserAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/alternatives",
    {
      slotId: "rp-user-slot-later",
      variantId: "rp-alt-user-slot",
      body: "This user-slot alternative should be rejected.",
    },
  );
  assert.equal(staleExplicitUserAlternative.status, 400);
  assert.match(
    staleExplicitUserAlternative.body.error.message,
    /assistant alternatives are only available for assistant message slots/,
  );

  const forkAtOldMessage = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/fork",
    {
      messageId: "rp-message-user-1",
      sessionId: "rp-browser-session-fork-old",
      displayName: "Browser RP Fork Old",
    },
  );
  assert.equal(
    forkAtOldMessage.status,
    201,
    JSON.stringify(forkAtOldMessage.body),
  );
  assert.equal(forkAtOldMessage.body.data.copied_message_count, 1);
  assert.equal(
    forkAtOldMessage.body.data.session.player_persona_id,
    "rp-player",
  );
  assert.deepEqual(forkAtOldMessage.body.data.session.active_layer_ids, [
    "rp-world",
  ]);

  const forkAtSelectedAlternative = await post(
    "/v1/admin/roleplay/sessions/rp-browser-session/fork",
    {
      messageId: "rp-message-assistant-alt-2",
      sessionId: "rp-browser-session-fork-selected",
      displayName: "Browser RP Fork Selected",
    },
  );
  assert.equal(
    forkAtSelectedAlternative.status,
    201,
    JSON.stringify(forkAtSelectedAlternative.body),
  );
  assert.equal(forkAtSelectedAlternative.body.data.copied_message_count, 2);
  assert.equal(
    forkAtSelectedAlternative.body.data.source_message_id,
    "rp-message-assistant-alt-2",
  );
  const forkedAlternatives = await get(
    "/v1/admin/roleplay/sessions/rp-browser-session-fork-selected/alternatives",
  );
  assert.equal(
    forkedAlternatives.status,
    200,
    JSON.stringify(forkedAlternatives.body),
  );
  assert.equal(
    forkedAlternatives.body.data.slot.active_variant.message.body,
    "The lantern reveals a serpent-and-rose crest in the dust.",
  );

  const reboundSession = await patch(
    "/v1/admin/roleplay/sessions/rp-browser-session",
    {
      playerPersonaId: null,
    },
  );
  assert.equal(reboundSession.status, 200, JSON.stringify(reboundSession.body));
  assert.equal(reboundSession.body.data.session.player_persona_id, undefined);
  assert.equal(
    reboundSession.body.data.session.player_persona_display_name,
    "Player",
  );
  assert.equal(
    reboundSession.body.data.session.player_persona_source,
    "fallback",
  );

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
