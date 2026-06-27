import assert from "node:assert/strict";
import {
  captureLoreFactTool,
  getLoreLayerConfigTool,
  listLoreLayersTool,
  manageLoreLayersTool,
  promoteLoreEntryTool,
  recallLoreTool,
  searchLoreTool,
} from "./index.js";

async function runSmoke(): Promise<void> {
  const bridge = new FakeLoreBridge();
  const context = {
    client: bridge,
    session: {
      sessionId: "session-moonlit",
      profileId: "profile-narrator",
    },
    now: () => "2026-06-27T12:01:00Z",
  };

  const manage = manageLoreLayersTool(context);
  const autoLayer = await manage.execute("create-auto", {
    action: "create",
    layerId: "layer-auto",
    profileId: "profile-narrator",
    name: "Auto capture",
    purpose: "story",
    writePolicy: "auto_capture",
  });
  assert.equal(autoLayer.details.ok, true);
  assert.equal(bridge.calls.at(-1)?.method, "createLoreLayer");

  const worldLayer = await manage.execute("create-world", {
    action: "create",
    layerId: "layer-world",
    profileId: "profile-narrator",
    name: "World lore",
    purpose: "world",
    writePolicy: "manual",
  });
  assert.equal(worldLayer.details.ok, true);

  const listed = await listLoreLayersTool(context).execute("list", {});
  assert.equal(listed.details.ok, true);
  assert.equal((listed.details.result as unknown[]).length, 2);

  bridge.configs.set("layer-auto", {
    config_id: "config-auto",
    layer_id: "layer-auto",
    default_token_budget: 500,
  });
  const config = await getLoreLayerConfigTool(context).execute("config", {
    layerId: "layer-auto",
  });
  assert.equal(config.details.ok, true);
  assert.equal(record(config.details.result).layer_id, "layer-auto");

  const capture = await captureLoreFactTool(context).execute("capture", {
    layerId: "layer-auto",
    recordId: "lore-silver-orchard",
    worldId: "world-moonlit",
    entityId: "entity-clockmaker",
    shapeId: "roleplay_lore_fact",
    title: "Silver Orchard",
    body: "The silver orchard blooms after the clockmaker sings.",
    content: { tags: ["orchard", "clockmaker"] },
    evidenceRefs: [
      {
        evidenceType: "transcript",
        refId: "message-1",
        label: "chat turn",
      },
    ],
    captureReason: "smoke test capture",
  });
  assert.equal(capture.details.ok, true);
  assert.equal(
    record(record(capture.details.result).record).record_id,
    "lore-silver-orchard",
  );
  assert.equal(bridge.calls.at(-1)?.method, "captureLoreFact");

  const promote = await promoteLoreEntryTool(context).execute("promote", {
    sourceLayerId: "layer-auto",
    sourceRecordId: "lore-silver-orchard",
    targetLayerId: "layer-world",
    newRecordId: "lore-silver-orchard-permanent",
    priority: 2,
  });
  assert.equal(promote.details.ok, true);
  assert.equal(
    record(record(promote.details.result).record).record_id,
    "lore-silver-orchard-permanent",
  );

  bridge.chatLayers.set("chat-moonlit", [
    { chat_id: "chat-moonlit", layer_id: "layer-world", enabled: true },
    { chat_id: "chat-moonlit", layer_id: "layer-auto", enabled: true },
  ]);
  const toggled = await manage.execute("toggle", {
    action: "toggle",
    chatId: "chat-moonlit",
    layerId: "layer-auto",
    enabled: false,
  });
  assert.equal(toggled.details.ok, true);
  const reordered = await manage.execute("reorder", {
    action: "reorder",
    chatId: "chat-moonlit",
    layerIds: ["layer-auto", "layer-world"],
  });
  assert.equal(reordered.details.ok, true);

  const search = await searchLoreTool(context).execute("search", {
    query: "silver orchard",
    worldId: "world-moonlit",
    chatId: "chat-moonlit",
    limit: 10,
  });
  assert.equal(search.details.ok, true);
  assert.ok((record(search.details.result).records as unknown[]).length >= 1);

  const recall = await recallLoreTool(context).execute("recall", {
    chatId: "chat-moonlit",
    queryText: "What blooms after the clockmaker sings?",
    activeSubjects: ["entity-clockmaker"],
    tokenBudget: 250,
    recordTrace: true,
    traceId: "trace-chat-moonlit-1",
  });
  assert.equal(recall.details.ok, true);
  assert.ok((record(recall.details.result).entries as unknown[]).length >= 1);
  assert.equal(bridge.calls.at(-1)?.method, "recallLore");

  const denied = await manage.execute("missing-layer", {
    action: "archive",
  });
  assert.equal(denied.details.ok, false);
  assert.equal(denied.details.reasonCode, "layer_id_missing");

  console.log(
    JSON.stringify(
      {
        calls: bridge.calls.map((call) => call.method),
        layers: (listed.details.result as unknown[]).length,
        captured: record(record(capture.details.result).record).record_id,
        promoted: record(record(promote.details.result).record).record_id,
        searchHits: (record(search.details.result).records as unknown[]).length,
        recallEntries: (record(recall.details.result).entries as unknown[])
          .length,
        denied: denied.details.reasonCode,
      },
      null,
      2,
    ),
  );
}

class FakeLoreBridge {
  readonly calls: Array<{ method: string; input: unknown }> = [];
  readonly layers = new Map<string, Record<string, unknown>>();
  readonly entries = new Map<string, Record<string, unknown>>();
  readonly layerEntries = new Map<string, Record<string, unknown>[]>();
  readonly chatLayers = new Map<string, Record<string, unknown>[]>();
  readonly configs = new Map<string, Record<string, unknown>>();

  async createLoreLayer(input: Record<string, unknown>) {
    this.recordCall("createLoreLayer", input);
    const layer = { ...input, is_archived: false };
    this.layers.set(String(input.layer_id), layer);
    return layer;
  }

  async updateLoreLayer(input: Record<string, unknown>) {
    this.recordCall("updateLoreLayer", input);
    const layer = {
      ...this.requiredLayer(input.layer_id),
      ...definedOnly(input),
    };
    this.layers.set(String(input.layer_id), layer);
    return layer;
  }

  async archiveLoreLayer(input: Record<string, unknown>) {
    this.recordCall("archiveLoreLayer", input);
    const layer = { ...this.requiredLayer(input.layer_id), is_archived: true };
    this.layers.set(String(input.layer_id), layer);
    return layer;
  }

  async listLoreLayers(profileId: string) {
    this.recordCall("listLoreLayers", profileId);
    return [...this.layers.values()].filter(
      (layer) => layer.profile_id === profileId,
    );
  }

  async getLoreLayerConfig(layerId: string) {
    this.recordCall("getLoreLayerConfig", layerId);
    return this.configs.get(layerId);
  }

  async captureLoreFact(input: Record<string, unknown>) {
    this.recordCall("captureLoreFact", input);
    const write = record(input.write);
    const entry = {
      ...write,
      status: "active",
      revision: 1,
      source: write.source,
    };
    this.entries.set(String(write.record_id), entry);
    const join = {
      layer_id: input.layer_id,
      record_id: write.record_id,
      is_constant: input.is_constant,
      priority: input.priority,
      record: entry,
    };
    this.pushLayerEntry(String(input.layer_id), join);
    return join;
  }

  async promoteLoreEntry(input: Record<string, unknown>) {
    this.recordCall("promoteLoreEntry", input);
    const source = this.entries.get(String(input.source_record_id));
    if (!source) throw new Error("source record missing");
    const promoted = {
      ...source,
      record_id: input.new_record_id,
      source: "human",
      revision: 1,
    };
    this.entries.set(String(input.new_record_id), promoted);
    const join = {
      layer_id: input.target_layer_id,
      record_id: input.new_record_id,
      is_constant: input.is_constant,
      priority: input.priority,
      record: promoted,
    };
    this.pushLayerEntry(String(input.target_layer_id), join);
    return join;
  }

  async getChatLayers(chatId: string) {
    this.recordCall("getChatLayers", chatId);
    return this.chatLayers.get(chatId) ?? [];
  }

  async toggleChatLayer(input: {
    chatId: string;
    layerId: string;
    enabled: boolean;
  }) {
    this.recordCall("toggleChatLayer", input);
    const layers = this.chatLayers.get(input.chatId) ?? [];
    for (const layer of layers) {
      if (layer.layer_id === input.layerId) layer.enabled = input.enabled;
    }
  }

  async reorderChatLayers(input: { chatId: string; layerIds: string[] }) {
    this.recordCall("reorderChatLayers", input);
    const existing = this.chatLayers.get(input.chatId) ?? [];
    this.chatLayers.set(
      input.chatId,
      input.layerIds.flatMap((layerId) =>
        existing.filter((layer) => layer.layer_id === layerId),
      ),
    );
  }

  async listEntriesByLayer(layerId: string) {
    this.recordCall("listEntriesByLayer", layerId);
    return this.layerEntries.get(layerId) ?? [];
  }

  async queryLoreEntries(input: Record<string, unknown>) {
    this.recordCall("queryLoreEntries", input);
    const query = String(input.query ?? "").toLowerCase();
    return [...this.entries.values()].filter(
      (entry) =>
        (!input.world_id || entry.world_id === input.world_id) &&
        String(entry.body ?? "")
          .toLowerCase()
          .includes(query),
    );
  }

  async recallLore(input: Record<string, unknown>) {
    this.recordCall("recallLore", input);
    const layerIds = (this.chatLayers.get(String(input.chat_id)) ?? [])
      .filter((layer) => layer.enabled !== false)
      .map((layer) => String(layer.layer_id));
    const entries = layerIds.flatMap((layerId) =>
      (this.layerEntries.get(layerId) ?? []).map((entry) => ({
        ...entry,
        score: 1,
        token_estimate: 12,
      })),
    );
    return {
      chat_id: input.chat_id,
      entries,
      entries_considered: entries.length,
      tokens_consumed: 12 * entries.length,
      token_budget: input.token_budget,
      trace: input.record_trace
        ? {
            trace_id: input.trace_id,
            entries_returned: entries.length,
          }
        : undefined,
    };
  }

  private requiredLayer(layerId: unknown): Record<string, unknown> {
    const layer = this.layers.get(String(layerId));
    if (!layer) throw new Error(`missing layer ${String(layerId)}`);
    return layer;
  }

  private pushLayerEntry(
    layerId: string,
    entry: Record<string, unknown>,
  ): void {
    this.layerEntries.set(layerId, [
      ...(this.layerEntries.get(layerId) ?? []),
      entry,
    ]);
  }

  private recordCall(method: string, input: unknown): void {
    this.calls.push({ method, input });
  }
}

await runSmoke();

function definedOnly(input: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(input).filter(([, value]) => value !== undefined),
  );
}

function record(value: unknown): Record<string, unknown> {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  return value as Record<string, unknown>;
}
