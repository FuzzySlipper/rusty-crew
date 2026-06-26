import assert from "node:assert/strict";
import {
  handleStorageQueryRequest,
  storageQueryCatalogTool,
  storageQueryExecuteTool,
  type AdminRouteResult,
  type StorageQueryCatalog,
  type StorageQueryContext,
  type StorageQueryResult,
} from "./index.js";

const moduleRegistryFixture = {
  source: "compiled_module_schema_registry",
  backendCapabilities: ["transactions", "json_documents"],
  modules: [
    {
      moduleId: "simple_kv",
      ownerCrate: "core_persistence",
      ownerModule: "simple_kv",
      descriptorVersion: 1,
      installedVersion: 1,
      migrationStatus: "installed",
      descriptorFingerprint: "fnv1a64:1234",
      installedDescriptorFingerprint: "fnv1a64:1234",
      installedAt: "2026-06-26T00:00:00Z",
      updatedAt: "2026-06-26T00:00:00Z",
      capabilityStatus: [],
      logicalStores: [{ storeName: "entries", description: "Simple entries" }],
      physicalTables: [
        {
          tableName: "entries",
          logicalStore: "entries",
          physicalTable: "module_simple_kv_entries",
          declaration: "owned",
        },
      ],
      physicalIndexes: [],
      retention: [],
      repositoryContracts: [],
      queryCatalogEntries: [],
      exportHooks: [],
      importHooks: [],
      migrationNotes: [],
      degradedReasons: [],
      blockedReasons: [],
    },
  ],
  orphanInstalledModules: [],
} satisfies Awaited<ReturnType<StorageQueryContext["bridge"]["storageSchema"]>>;

const bridge = {
  async storageDiagnostics() {
    return {
      backend: "sqlite",
      backendLabel: "SQLite WAL",
      schemaVersion: 20,
      supportedSchemaVersion: 20,
      migrations: [],
      size: {
        databaseBytes: 8192,
        pageCount: 2,
        pageSizeBytes: 4096,
        freelistPages: 0,
        freelistBytes: 0,
        walBytes: 0,
      },
      tableCounts: [
        { table: "sessions", rows: 2 },
        { table: "queued_messages", rows: 4 },
        { table: "profile_memories", rows: 1 },
      ],
      capabilities: [],
      repositoryGroups: [
        {
          groupId: "queues_messages",
          label: "Queues And Messages",
          correctnessSensitive: true,
          backendRequirements: [
            {
              capability: "transactions",
              required: true,
              detail: "queue writes must be atomic",
            },
          ],
          notes: ["Queue TTL keeps stale pending work from resurfacing."],
        },
      ],
      moduleRegistry: moduleRegistryFixture,
      indexChecks: [],
      searchHealthy: true,
      pressure: false,
    };
  },
  async storageSchema() {
    return moduleRegistryFixture;
  },
  async searchRuntime(query) {
    assert.equal(query.rowType, "queue_message");
    assert.equal(query.limit, 2);
    return [
      {
        rowType: "queue_message",
        rowKey: "queue-1",
        sessionId: "session-alpha",
        recordedAt: "2026-06-25T12:00:00.000Z",
        title: "queued wake",
        body: "wake payload",
      },
    ];
  },
  async listProfileMemory(query) {
    assert.equal(query.profileId, "rusty-crew-runner");
    return [
      {
        profileId: "rusty-crew-runner",
        targetType: "profile",
        targetId: "",
        key: "working_style",
        content: "steady",
        metadataJson: "{}",
        revision: 1,
        createdAt: "2026-06-25T12:00:00.000Z",
        updatedAt: "2026-06-25T12:00:00.000Z",
      },
    ];
  },
  async listSimpleKv(query) {
    assert.equal(query.scopeType, "profile");
    assert.equal(query.scopeId, "rusty-crew-runner");
    assert.equal(query.keyPrefix, "work");
    assert.equal(query.includeExpired, false);
    assert.equal(query.expiredOnly, false);
    assert.equal(query.limit, 10);
    return [
      {
        scopeType: "profile",
        scopeId: "rusty-crew-runner",
        key: "working_style",
        valueJson: JSON.stringify({ style: "steady" }),
        revision: 1,
        createdAt: "2026-06-26T00:00:00Z",
        updatedAt: "2026-06-26T00:00:00Z",
      },
    ];
  },
  async queryConversationBranches(query) {
    assert.deepEqual(query, {
      session_id: "session-alpha",
      page: { limit: 1, offset: 0 },
    });
    return [
      {
        branch_id: "branch-default",
        session_id: "session-alpha",
        label: "Default",
      },
    ];
  },
  async queryRuntimeCounters(query) {
    assert.equal(query.scopeType, "session");
    return [
      {
        scopeType: "session",
        scopeId: "session-alpha",
        counterName: "wakes",
        value: 3,
        updatedAt: "2026-06-25T12:00:00.000Z",
      },
    ];
  },
} satisfies StorageQueryContext["bridge"];

const context: StorageQueryContext = { bridge };

const catalog = await handleStorageQueryRequest(
  {
    method: "GET",
    url: "/v1/admin/storage/query-catalog",
    requestId: "req-catalog",
  },
  context,
);
assert.equal(catalog.status, 200);
const catalogData = okData<StorageQueryCatalog>(catalog);
assert.ok(catalogData.items.some((item) => item.id === "runtime.search"));
assert.ok(catalogData.items.some((item) => item.id === "storage.schema"));
const simpleKvDescriptor = catalogData.items.find(
  (item) => item.id === "simple_kv.entries",
);
assert.equal(simpleKvDescriptor?.module?.moduleId, "simple_kv");

const storageSchema = await handleStorageQueryRequest(
  {
    method: "GET",
    url: "/v1/admin/storage/schema",
    requestId: "req-schema",
  },
  context,
);
assert.equal(storageSchema.status, 200);
assert.equal(
  okData<Awaited<ReturnType<typeof bridge.storageSchema>>>(storageSchema)
    .modules[0]?.moduleId,
  "simple_kv",
);

const tableCounts = await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/storage.table_counts",
    body: { table: "queued_messages", limit: 10 },
    requestId: "req-table",
  },
  context,
);
assert.equal(tableCounts.status, 200);
assert.deepEqual(okData<StorageQueryResult>(tableCounts).items, [
  { table: "queued_messages", rows: 4 },
]);

const search = await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/runtime.search",
    body: { query: "wake", rowType: "queue_message", limit: 2 },
    requestId: "req-search",
  },
  context,
);
assert.equal(search.status, 200);
assert.equal(okData<StorageQueryResult>(search).total, 1);

const memoryTool = storageQueryExecuteTool(context);
const kv = await memoryTool.execute("call-kv", {
  queryId: "simple_kv.entries",
  input: {
    scopeType: "profile",
    scopeId: "rusty-crew-runner",
    keyPrefix: "work",
    limit: 10,
  },
});
assert.ok(!("ok" in kv.details));
assert.equal(kv.details.query_id, "simple_kv.entries");
assert.equal(kv.details.items?.length, 1);

const memory = await memoryTool.execute("call-memory", {
  queryId: "profile.memory",
  input: { profileId: "rusty-crew-runner" },
});
assert.ok(!("ok" in memory.details));
assert.equal(memory.details.query_id, "profile.memory");
assert.equal(memory.details.items?.length, 1);

const catalogTool = await storageQueryCatalogTool().execute("call-catalog", {});
assert.equal(catalogTool.details.total, 7);

const invalid = await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/runtime.search",
    body: { query: "", limit: 2 },
    requestId: "req-invalid",
  },
  context,
);
assert.equal(invalid.status, 400);
assert.equal(invalid.body.ok, false);
assert.equal(
  invalid.body.ok ? undefined : invalid.body.error.reason_code,
  "invalid_string_parameter",
);

const invalidKv = await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/simple_kv.entries",
    body: {
      scopeType: "profile",
      scopeId: "rusty-crew-runner",
      expiryStatus: "expired",
    },
    requestId: "req-invalid-kv",
  },
  context,
);
assert.equal(invalidKv.status, 400);
assert.equal(
  invalidKv.body.ok ? undefined : invalidKv.body.error.reason_code,
  "now_required_for_expired_entries",
);

const unknown = await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/nope",
    body: {},
    requestId: "req-unknown",
  },
  context,
);
assert.equal(unknown.status, 404);

await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/conversation.branches",
    body: { sessionId: "session-alpha", limit: 1 },
  },
  context,
);

await handleStorageQueryRequest(
  {
    method: "POST",
    url: "/v1/admin/storage/query/runtime.counters",
    body: { scopeType: "session" },
  },
  context,
);

console.log("smoke-storage-query-catalog ok");

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  return result.body.data as T;
}
