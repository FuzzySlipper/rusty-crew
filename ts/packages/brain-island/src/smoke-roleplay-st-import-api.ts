import assert from "node:assert/strict";
import {
  readFileSync,
  mkdtempSync,
  mkdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const exampleDir = "/home/stash/st-example";
const profileId = "st-import-profile";
const importId = "dark-xavier-st-example";
const sessionId = "dark-xavier-st-session";
const root = mkdtempSync(join(tmpdir(), "rusty-crew-st-import-"));
const port = await openPort();
const token = "roleplay-st-import-smoke-token";

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
  bridge: await loadNativeBridge(),
});

try {
  const plan = buildPlan();
  const imported = await post("/v1/admin/roleplay/imports/st-packet", plan);
  assert.equal(imported.status, 200, JSON.stringify(imported.body));
  assert.equal(imported.body.data.counts.characters, 1);
  assert.equal(imported.body.data.counts.personas, 1);
  assert.equal(imported.body.data.counts.loreEntries, 24);
  assert.equal(imported.body.data.counts.messages, 71);
  assert.equal(imported.body.data.counts.assistantVariantRows, 36);
  assert.equal(imported.body.data.counts.assistantMultiSwipeRows, 9);
  assert.equal(imported.body.data.counts.variants, 82);

  const loreEntries = await get(
    "/v1/admin/roleplay/lore/layers/st-lads-philos/entries",
  );
  assert.equal(loreEntries.status, 200, JSON.stringify(loreEntries.body));
  assert.equal(loreEntries.body.data.entries.length, 24);

  const slots = await get(
    `/v1/chat/sessions/${sessionId}/slots?include_alternates=true`,
  );
  assert.equal(slots.status, 200, JSON.stringify(slots.body));
  assert.equal(slots.body.data.items.length, 71);
  const assistantSlots = slots.body.data.items.filter(
    (slot: any) => slot.primary.message.author_role === "assistant",
  );
  assert.equal(assistantSlots.length, 36);
  assert.equal(
    assistantSlots.filter((slot: any) => slot.alternates.length > 0).length,
    9,
  );
  const variantCount = slots.body.data.items.reduce(
    (total: number, slot: any) => total + 1 + slot.alternates.length,
    0,
  );
  assert.equal(variantCount, 82);
  const firstAssistant = assistantSlots[0];
  assert.equal(
    firstAssistant.primary.message.metadata_json.source,
    "st_packet_import",
  );
  assert.equal(
    typeof firstAssistant.primary.message.metadata_json.st_extra,
    "object",
  );

  console.log(
    JSON.stringify(
      {
        importId,
        loreEntries: loreEntries.body.data.entries.length,
        messages: slots.body.data.items.length,
        assistantVariantRows: assistantSlots.length,
        variants: variantCount,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop();
  rmSync(root, { recursive: true, force: true });
}

function buildPlan(): Record<string, unknown> {
  const manifest = readJson("manifest.json");
  const card = readJson("Character Card - Crown Prince Xavier.json");
  const cardData = (card.data ?? card) as Record<string, any>;
  const persona = readJson("Persona - Kopis Valliren.json");
  const lorebook = readJson("Lorebook - LaDS_Philos.json");
  const rows = readFileSync(
    join(exampleDir, "Transcript - Crown Prince Xavier.jsonl"),
    "utf8",
  )
    .trim()
    .split(/\n/)
    .slice(1)
    .map((line) => JSON.parse(line));
  return {
    profileId,
    importId,
    provenance: {
      source: "st-example",
      package: manifest.package,
      generated: manifest.generated,
      manifestSha256: manifest.files,
    },
    rawSource: {
      manifest,
      presetFile: "Preset - Ava's Special.json",
      renderedPromptFile: "Rendered Prompt Export.txt",
    },
    character: {
      id: "st-crown-prince-xavier",
      name: cardData.name,
      description: cardData.description,
      personality: cardData.personality,
      scenario: cardData.scenario,
      firstMessage: cardData.first_mes,
      alternateGreetings: cardData.alternate_greetings ?? [],
      exampleMessages: [cardData.mes_example].filter(Boolean),
      tags: cardData.tags ?? [],
      rawMetadata: {
        spec: card.spec,
        spec_version: card.spec_version,
        creator: cardData.creator,
        extensions: cardData.extensions,
      },
    },
    persona: {
      id: "st-kopis-valliren",
      displayName: persona.name,
      description: persona.description,
      notes: persona.comment,
      rawMetadata: {
        spec: persona.spec,
        spec_version: persona.spec_version,
      },
    },
    loreLayer: {
      layerId: "st-lads-philos",
      name: "LaDS_Philos",
      description: "Imported SillyTavern lorebook from the ST example corpus.",
      purpose: "mixed",
      writePolicy: "manual",
    },
    loreEntries: Object.values(lorebook.entries).map((entry: any) => ({
      recordId: `st-lore-${entry.uid ?? entry.id}`,
      title: entry.comment || entry.name || `Lore ${entry.uid ?? entry.id}`,
      body: entry.content,
      worldId: profileId,
      entityId: entry.comment || entry.name,
      canonStatus: "draft",
      visibility: "public",
      primaryKeys: entry.key ?? entry.keys ?? [],
      secondaryKeys: entry.keysecondary ?? entry.secondary_keys ?? [],
      constant: entry.constant,
      enabled: entry.disable === true ? false : entry.enabled !== false,
      insertionOrder: entry.insertion_order ?? entry.order,
      probability:
        typeof entry.probability === "number" ? entry.probability / 100 : 1,
      rawMetadata: entry,
    })),
    session: {
      sessionId,
      displayName: "Dark Xavier ST Example",
    },
    transcriptRows: rows.map((row: any, index: number) => ({
      role: row.is_system ? "system" : row.is_user ? "user" : "assistant",
      name: row.name,
      send_date: row.send_date,
      body: row.mes,
      swipe_id: row.swipe_id,
      swipes: Array.isArray(row.swipes) ? row.swipes : undefined,
      swipe_info: row.swipe_info,
      extra: row.extra,
      metadata: {
        source_index: index,
        is_user: row.is_user,
        is_system: row.is_system,
      },
    })),
  };
}

function readJson(fileName: string): any {
  return JSON.parse(readFileSync(join(exampleDir, fileName), "utf8"));
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

function writeRuntimeConfig(dataDir: string): void {
  const configDir = join(dataDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId }],
        sessions: [
          {
            sessionId: "st-import-seed-session",
            agentId: profileId,
            profileId,
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
    join(profilesDir, `${profileId}.json`),
    JSON.stringify(
      {
        profileId,
        displayName: "ST Import Profile",
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
