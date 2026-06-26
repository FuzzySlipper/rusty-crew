import assert from "node:assert/strict";
import type { MemorySpaceDescriptor } from "@rusty-crew/contracts";
import type { NativeProfileMemoryRecord } from "@rusty-crew/native-bridge";
import {
  createMemorySpaceToolResolver,
  handleMemorySpaceAdminRequest,
  memorySpaceCatalogTool,
  memorySpaceReadTool,
  type AdminRouteResult,
  type MemorySpaceReadContext,
} from "./index.js";

const descriptor = profileDenseDescriptor();
const records: NativeProfileMemoryRecord[] = [
  profileMemory("working_style", "steady"),
  profileMemory("handoff_style", "concise"),
];

const bridge: MemorySpaceReadContext["bridge"] = {
  async listMemorySpaceDescriptors() {
    return [descriptor];
  },
  async listProfileMemory(query) {
    assert.equal(query.profileId, "rusty-crew-runner");
    assert.equal(query.targetType ?? "profile", "profile");
    assert.equal(query.targetId ?? "", "");
    const offset = query.offset ?? 0;
    const limit = query.limit ?? records.length;
    return records.slice(offset, offset + limit);
  },
  async getProfileMemory(input) {
    assert.equal(input.profileId, "rusty-crew-runner");
    assert.equal(input.targetType, "profile");
    assert.equal(input.targetId ?? "", "");
    return records.find((record) => record.key === input.key);
  },
};

const context: MemorySpaceReadContext = { bridge };

const catalog = await handleMemorySpaceAdminRequest(
  { method: "GET", url: "/v1/admin/memory/spaces", requestId: "catalog" },
  context,
);
assert.equal(catalog.status, 200);
assert.equal(okData<{ total: number }>(catalog).total, 1);

const descriptorRead = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/profile_dense",
    requestId: "descriptor",
  },
  context,
);
assert.equal(descriptorRead.status, 200);
assert.equal(
  okData<MemorySpaceDescriptor>(descriptorRead).space_id,
  "profile_dense",
);

const recordList = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/profile_dense/records?profileId=rusty-crew-runner&limit=1",
    requestId: "records",
  },
  context,
);
assert.equal(recordList.status, 200);
const recordListData = okData<{
  items: NativeProfileMemoryRecord[];
  limit: number;
  nextOffset?: number;
  read_only: true;
}>(recordList);
assert.equal(recordListData.read_only, true);
assert.equal(recordListData.items.length, 1);
assert.equal(recordListData.items[0]?.key, "working_style");
assert.equal(recordListData.limit, 1);
assert.equal(recordListData.nextOffset, 1);

const recordRead = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/profile_dense/records/handoff_style?profileId=rusty-crew-runner",
    requestId: "record",
  },
  context,
);
assert.equal(recordRead.status, 200);
assert.equal(
  okData<{ item?: NativeProfileMemoryRecord }>(recordRead).item?.key,
  "handoff_style",
);

const missingSpace = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/den_memory",
    requestId: "missing",
  },
  context,
);
assert.equal(missingSpace.status, 404);
assert.equal(errorReason(missingSpace), "memory_space_not_found");

const readonly = await handleMemorySpaceAdminRequest(
  { method: "POST", url: "/v1/admin/memory/spaces", requestId: "readonly" },
  context,
);
assert.equal(readonly.status, 405);
assert.equal(errorReason(readonly), "memory_space_read_only");

const invalidQuery = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/profile_dense/records?profileId=rusty-crew-runner&targetType=user",
    requestId: "invalid",
  },
  context,
);
assert.equal(invalidQuery.status, 400);
assert.equal(errorReason(invalidQuery), "target_id_required");

const catalogTool = await memorySpaceCatalogTool(context).execute(
  "catalog",
  {},
);
if (!("total" in catalogTool.details)) {
  throw new Error("expected memory-space catalog tool result");
}
assert.equal(catalogTool.details.total, 1);

const readTool = memorySpaceReadTool({
  context,
  session: { profileId: "rusty-crew-runner" as never },
});
const toolList = await readTool.execute("list", { spaceId: "profile_dense" });
if (!("read_only" in toolList.details)) {
  throw new Error("expected memory-space record list tool result");
}
assert.equal(toolList.details.read_only, true);
assert.equal(
  "items" in toolList.details ? toolList.details.items.length : 0,
  2,
);
const toolRead = await readTool.execute("read", {
  spaceId: "profile_dense",
  key: "working_style",
});
assert.equal(
  "item" in toolRead.details ? toolRead.details.item?.content : "",
  "steady",
);
const toolMissing = await readTool.execute("missing", { spaceId: "unknown" });
assert.equal(
  "ok" in toolMissing.details ? toolMissing.details.ok : true,
  false,
);
assert.equal(
  "reason_code" in toolMissing.details ? toolMissing.details.reason_code : "",
  "memory_space_not_found",
);

const resolvedTools = createMemorySpaceToolResolver(context)({
  wake: {
    state: {
      session: { profileId: "rusty-crew-runner", toolProfile: { tools: [] } },
    },
  } as never,
  tools: [],
});
assert.deepEqual(
  resolvedTools.map((tool) => tool.name),
  ["memory_space_catalog", "memory_space_read"],
);

console.log("smoke-memory-space-api ok");

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  if (!result.body.ok) throw new Error("expected admin route success");
  return result.body.data as T;
}

function errorReason(result: AdminRouteResult): string {
  assert.equal(result.body.ok, false);
  if (result.body.ok) throw new Error("expected admin route failure");
  return result.body.error.reason_code;
}

function profileMemory(
  key: string,
  content: string,
): NativeProfileMemoryRecord {
  return {
    profileId: "rusty-crew-runner",
    targetType: "profile",
    targetId: "",
    key,
    content,
    metadataJson: "{}",
    revision: 1,
    createdAt: "2026-06-26T00:00:00.000Z",
    updatedAt: "2026-06-26T00:00:00.000Z",
  };
}

function profileDenseDescriptor(): MemorySpaceDescriptor {
  return {
    space_id: "profile_dense" as never,
    schema_version: 1,
    module_id: "runtime_memory",
    description: "Existing dense profile memory.",
    record_shapes: [
      {
        shape_id: "profile_dense_item" as never,
        version: 1,
        description: "Dense profile memory record.",
        fields: [
          {
            field_name: "key",
            field_type: "string",
            required: true,
            description: "Memory key.",
          },
          {
            field_name: "content",
            field_type: "markdown",
            required: true,
            description: "Memory content.",
          },
          {
            field_name: "revision",
            field_type: "integer",
            required: true,
            description: "Expected revision token.",
          },
        ],
      },
    ],
    scope_model: {
      allowed_scopes: ["profile", "user"],
      primary_scope: "profile",
    },
    visibility_model: "profile_local",
    retrieval_strategies: ["direct_lookup", "query_search"],
    indexing: {
      required_capabilities: [
        "profile_target_key_lookup",
        "expected_revision_conflicts",
      ],
      optional_capabilities: [
        "profile_scoped_listing",
        "cap_max_records_per_profile_64",
        "cap_max_key_bytes_128",
        "cap_max_content_bytes_8192",
      ],
    },
    prompt_policy: "summary_context",
    write_policy: {
      default_mode: "direct_write",
      operation_policies: [
        {
          operation: "add",
          governance_mode: "direct_write",
          requires_expected_revision: false,
        },
        {
          operation: "replace",
          governance_mode: "direct_write",
          requires_expected_revision: true,
        },
        {
          operation: "remove",
          governance_mode: "direct_write",
          requires_expected_revision: true,
        },
      ],
    },
    conflict_policy: "expected_revision",
    operations: ["read", "list", "add", "replace", "remove"],
    provenance_policy: {
      required_evidence: ["wake"],
      source_required: false,
      rationale_required: false,
    },
    retention_policy: "manual_only",
    diagnostics: {
      expose_catalog: true,
      expose_record_counts: true,
      expose_policy_decisions: true,
    },
    export_import: {
      export_supported: true,
      import_supported: true,
      import_governance_mode: "manual_review",
    },
  };
}
