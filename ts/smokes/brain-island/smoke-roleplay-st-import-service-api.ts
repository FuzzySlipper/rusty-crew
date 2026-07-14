import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createNetServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";
import { buildStExampleImportPlan } from "../../packages/brain-island/src/roleplay-st-example-fixture.js";

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
  const plan = buildStExampleImportPlan({ profileId, importId, sessionId });
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
  const assistantVariantCount = assistantSlots.reduce(
    (total: number, slot: any) => total + 1 + slot.alternates.length,
    0,
  );
  assert.equal(assistantVariantCount, 47);
  const transcriptRows = (plan as any).transcriptRows as any[];
  assert.equal(transcriptRows.length, slots.body.data.items.length);
  for (const [index, slot] of slots.body.data.items.entries()) {
    const sourceRow = transcriptRows[index];
    const variants = [slot.primary, ...slot.alternates].sort(
      (left: any, right: any) => left.ordinal - right.ordinal,
    );
    const sourceBodies =
      Array.isArray(sourceRow.swipes) && sourceRow.swipes.length > 0
        ? sourceRow.swipes
        : [sourceRow.body];
    const expectedActiveOrdinal = Math.min(
      Math.max(Number(sourceRow.swipe_id ?? 0), 0),
      sourceBodies.length - 1,
    );
    const activeVariantId = slot.active_variant_id ?? slot.primary_variant_id;
    const activeVariant = variants.find(
      (variant: any) => variant.variant_id === activeVariantId,
    );
    assert.ok(activeVariant, `row ${index} active variant is readable`);
    assert.equal(activeVariant.ordinal, expectedActiveOrdinal);
    assert.equal(
      activeVariant.message.body,
      sourceBodies[expectedActiveOrdinal],
    );
    assert.equal(
      activeVariant.message.metadata_json.source_index,
      sourceRow.metadata.source_index,
    );
    assert.equal(activeVariant.message.metadata_json.st_name, sourceRow.name);
    assert.equal(activeVariant.message.created_at, sourceRow.send_date);
    assert.deepEqual(
      activeVariant.message.metadata_json.st_source_metadata,
      JSON.parse(JSON.stringify(sourceRow.metadata)),
    );
    for (const variant of variants) {
      assert.equal(variant.message.body, sourceBodies[variant.ordinal]);
      if (Array.isArray(sourceRow.swipe_info)) {
        assert.deepEqual(
          variant.message.metadata_json.variant_metadata,
          sourceRow.swipe_info[variant.ordinal],
        );
      }
    }
  }
  const firstAssistant = assistantSlots[0];
  assert.equal(firstAssistant.primary.message.metadata_json.source_index, 1);
  assert.equal(
    firstAssistant.primary.message.metadata_json.source,
    "st_packet_import",
  );
  assert.equal(
    typeof firstAssistant.primary.message.metadata_json.st_extra,
    "object",
  );
  const reasoningVariant = assistantSlots
    .flatMap((slot: any) => [slot.primary, ...slot.alternates])
    .find(
      (variant: any) =>
        typeof variant.message.metadata_json.variant_metadata?.extra
          ?.reasoning === "string",
    );
  assert.ok(
    reasoningVariant,
    "reasoning-bearing assistant variant is readable",
  );
  assert.equal(
    typeof reasoningVariant.message.metadata_json.variant_metadata.extra.model,
    "string",
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
        modelConfig: {
          provider: "local",
          modelName: "deepseek-flash",
          baseUrl: "http://127.0.0.1:18082/v1",
        },
        brain: { module: "chat-completions" },
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
