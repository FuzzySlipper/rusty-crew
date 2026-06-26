import assert from "node:assert/strict";
import type { MemorySpaceDescriptor } from "@rusty-crew/contracts";
import type {
  NativeProfileMemoryRecord,
  NativeSessionMemoryRecord,
} from "@rusty-crew/native-bridge";
import {
  createMemorySpaceToolResolver,
  handleMemorySpaceAdminRequest,
  memorySpaceCatalogTool,
  memorySpaceReadTool,
  type AdminRouteResult,
  type MemorySpaceReadContext,
} from "./index.js";

const descriptor = profileDenseDescriptor();
const sessionDescriptor = sessionMemoryDescriptor();
const records: NativeProfileMemoryRecord[] = [
  profileMemory("working_style", "steady"),
  profileMemory("handoff_style", "concise"),
];
const sessionRecords: NativeSessionMemoryRecord[] = [
  sessionMemory("session-fact-one", "session_fact"),
  sessionMemory("session-summary-one", "session_summary"),
];

const bridge: MemorySpaceReadContext["bridge"] = {
  async listMemorySpaceDescriptors() {
    return [descriptor, sessionDescriptor];
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
  async querySessionMemoryRecords(query) {
    assert.equal(query.session_id, "session-alpha");
    assert.equal(query.shape_id ?? "", "");
    const offset = query.page?.offset ?? 0;
    const limit = query.page?.limit ?? sessionRecords.length;
    return sessionRecords.slice(offset, offset + limit);
  },
  async buildSessionMemoryPromptContext(query) {
    assert.equal(query.session_id, "session-alpha");
    const offset = query.page?.offset ?? 0;
    const limit = query.page?.limit ?? sessionRecords.length;
    const selected = sessionRecords.slice(offset, offset + limit);
    return {
      records: selected,
      diagnostics: {
        descriptor_id: "session_memory",
        descriptor_schema_version: 1,
        session_id: "session-alpha",
        active_branch_id: query.active_branch_id,
        selected_records: selected.map((record) => ({
          record_id: record.record_id,
          shape_id: record.shape.shape_id,
        })),
        excluded_counts: {
          wrong_branch: 0,
          sibling_branch: 0,
          tool_only: 0,
          archived: 0,
          superseded: 0,
          limit_exceeded: 0,
          policy_disabled: 0,
        },
        character_estimate: 120,
        token_estimate: 30,
        context_policy: "summary_context",
      },
    };
  },
  async saveMemoryProposal(proposal) {
    return {
      proposal,
      status: "pending_review",
      selected_governance_mode: "curator_route",
      created_at: "2026-06-26T00:00:00.000Z",
      updated_at: "2026-06-26T00:00:00.000Z",
    };
  },
  async listMemoryProposals(query) {
    assert.equal(query.space_id ?? "profile_dense", "profile_dense");
    return [];
  },
  async recordMemoryGovernanceDecision(decision) {
    return {
      ...decision,
      decided_at: decision.decided_at ?? "2026-06-26T00:00:00.000Z",
    };
  },
};

const context: MemorySpaceReadContext = { bridge };

const catalog = await handleMemorySpaceAdminRequest(
  { method: "GET", url: "/v1/admin/memory/spaces", requestId: "catalog" },
  context,
);
assert.equal(catalog.status, 200);
const catalogData = okData<{ total: number; items: MemorySpaceDescriptor[] }>(
  catalog,
);
assert.equal(catalogData.total, 2);
assert.deepEqual(
  catalogData.items
    .find((item) => item.space_id === "session_memory")
    ?.record_shapes.map((shape) => shape.shape_id),
  ["session_fact", "session_summary", "branch_summary", "user_choice"],
);

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

const sessionMemoryList = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/session_memory/records?sessionId=session-alpha&limit=1",
    requestId: "session-memory-records",
  },
  context,
);
assert.equal(sessionMemoryList.status, 200);
const sessionMemoryListData = okData<{
  items: NativeSessionMemoryRecord[];
  limit: number;
  nextOffset?: number;
}>(sessionMemoryList);
assert.equal(sessionMemoryListData.items.length, 1);
assert.equal(sessionMemoryListData.items[0]?.record_id, "session-fact-one");
assert.equal(sessionMemoryListData.nextOffset, 1);

const sessionMemoryPromptList = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/session_memory/records?sessionId=session-alpha&activeBranchId=branch-active&promptContextOnly=true",
    requestId: "session-memory-prompt-records",
  },
  context,
);
assert.equal(sessionMemoryPromptList.status, 200);
assert.equal(
  okData<{ diagnostics?: { descriptor_id: string } }>(sessionMemoryPromptList)
    .diagnostics?.descriptor_id,
  "session_memory",
);

const invalidSessionMemoryQuery = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/spaces/session_memory/records?profileId=rusty-crew-runner",
    requestId: "invalid-session-memory",
  },
  context,
);
assert.equal(invalidSessionMemoryQuery.status, 400);
assert.equal(
  errorReason(invalidSessionMemoryQuery),
  "missing_required_parameter",
);

const proposalCreate = await handleMemorySpaceAdminRequest(
  {
    method: "POST",
    url: "/v1/admin/memory/proposals",
    requestId: "proposal-create",
    body: {
      proposal_id: "proposal_one",
      space_id: "profile_dense",
      operation: "candidate_only",
      scope: { scope_type: "profile", scope_id: "rusty-crew-runner" },
      shape: { shape_id: "profile_dense_item", version: 1 },
      content: { key: "working_style", content: "steady" },
      evidence_refs: [{ evidence_type: "wake", ref_id: "wake-alpha" }],
      confidence: 0.82,
      governance_mode: "direct_write",
      source: "in_wake_tool",
      dedupe_key: "profile_dense:working_style",
    },
  },
  context,
);
assert.equal(proposalCreate.status, 200);
assert.equal(
  okData<{ status: string; selected_governance_mode: string }>(proposalCreate)
    .selected_governance_mode,
  "curator_route",
);

const proposalList = await handleMemorySpaceAdminRequest(
  {
    method: "GET",
    url: "/v1/admin/memory/proposals?spaceId=profile_dense&status=pending_review",
    requestId: "proposal-list",
  },
  context,
);
assert.equal(proposalList.status, 200);

const proposalDecision = await handleMemorySpaceAdminRequest(
  {
    method: "POST",
    url: "/v1/admin/memory/proposals/proposal_one/decisions",
    requestId: "proposal-decision",
    body: {
      decision_id: "decision_one",
      proposal_id: "proposal_one",
      decision: "approved",
      actor: "operator",
      source: "human",
      evidence_refs: [{ evidence_type: "ui", ref_id: "admin" }],
      policy_mode: "manual_review",
    },
  },
  context,
);
assert.equal(proposalDecision.status, 200);

const proposalDecisionMismatch = await handleMemorySpaceAdminRequest(
  {
    method: "POST",
    url: "/v1/admin/memory/proposals/proposal_one/decisions",
    requestId: "proposal-decision-mismatch",
    body: {
      decision_id: "decision_mismatch",
      proposal_id: "proposal_two",
      decision: "approved",
      actor: "operator",
      source: "human",
      evidence_refs: [],
      policy_mode: "manual_review",
    },
  },
  context,
);
assert.equal(proposalDecisionMismatch.status, 400);
assert.equal(
  errorReason(proposalDecisionMismatch),
  "memory_proposal_id_mismatch",
);

const catalogTool = await memorySpaceCatalogTool(context).execute(
  "catalog",
  {},
);
if (!("total" in catalogTool.details)) {
  throw new Error("expected memory-space catalog tool result");
}
assert.equal(catalogTool.details.total, 2);

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

function sessionMemory(
  recordId: string,
  shapeId: string,
): NativeSessionMemoryRecord {
  return {
    record_id: recordId,
    session_id: "session-alpha",
    scope: { scope_type: "session", scope_id: "session-alpha" },
    branch_id: undefined,
    shape: { shape_id: shapeId, version: 1 },
    status: "active",
    revision: 1,
    content: {
      record_id: recordId,
      content: "Session memory fixture.",
      fact_kind: "preference",
      confidence: 0.9,
      source_summary: "Fixture",
      created_at: "2026-06-26T00:00:00.000Z",
      updated_at: "2026-06-26T00:00:00.000Z",
    },
    evidence_refs: [{ evidence_type: "wake", ref_id: "wake-alpha" }],
    source: "capture_producer",
    confidence: 0.9,
    durability_rationale: "Fixture record for read API smoke.",
    created_at: "2026-06-26T00:00:00.000Z",
    updated_at: "2026-06-26T00:00:00.000Z",
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

function sessionMemoryDescriptor(): MemorySpaceDescriptor {
  const requiredString = (fieldName: string) => ({
    field_name: fieldName,
    field_type: "string" as const,
    required: true,
    description: `${fieldName} field.`,
  });
  return {
    space_id: "session_memory" as never,
    schema_version: 1,
    module_id: "runtime_memory",
    description:
      "Crew-owned session and branch memory; not Den memory and not transcript storage.",
    record_shapes: [
      {
        shape_id: "session_fact" as never,
        version: 1,
        description: "Durable session fact.",
        fields: [
          requiredString("record_id"),
          { ...requiredString("content"), field_type: "markdown" },
          requiredString("fact_kind"),
          { ...requiredString("confidence"), field_type: "float" },
          requiredString("source_summary"),
          { ...requiredString("created_at"), field_type: "timestamp" },
          { ...requiredString("updated_at"), field_type: "timestamp" },
        ],
      },
      {
        shape_id: "session_summary" as never,
        version: 1,
        description: "Session summary.",
        fields: [
          requiredString("record_id"),
          { ...requiredString("summary"), field_type: "markdown" },
          requiredString("coverage_start"),
          requiredString("coverage_end"),
          requiredString("summary_kind"),
          { ...requiredString("created_at"), field_type: "timestamp" },
          { ...requiredString("updated_at"), field_type: "timestamp" },
        ],
      },
      {
        shape_id: "branch_summary" as never,
        version: 1,
        description: "Branch summary.",
        fields: [
          requiredString("record_id"),
          { ...requiredString("summary"), field_type: "markdown" },
          requiredString("branch_id"),
          requiredString("head_message_id"),
          requiredString("coverage_start"),
          requiredString("coverage_end"),
          { ...requiredString("created_at"), field_type: "timestamp" },
          { ...requiredString("updated_at"), field_type: "timestamp" },
        ],
      },
      {
        shape_id: "user_choice" as never,
        version: 1,
        description: "User choice.",
        fields: [
          requiredString("record_id"),
          { ...requiredString("choice"), field_type: "markdown" },
          requiredString("choice_kind"),
          { ...requiredString("chosen_at"), field_type: "timestamp" },
          requiredString("status"),
          { ...requiredString("created_at"), field_type: "timestamp" },
          { ...requiredString("updated_at"), field_type: "timestamp" },
        ],
      },
    ],
    scope_model: {
      allowed_scopes: ["session", "conversation_branch"],
      primary_scope: "session",
    },
    visibility_model: "session_scoped",
    retrieval_strategies: [
      "direct_lookup",
      "recency",
      "branch_aware",
      "query_search",
    ],
    indexing: {
      required_capabilities: ["session_scope_lookup"],
      optional_capabilities: ["branch_aware_lookup", "query_search"],
    },
    prompt_policy: "summary_context",
    write_policy: {
      default_mode: "candidate",
      operation_policies: [
        {
          operation: "add",
          governance_mode: "candidate",
          requires_expected_revision: false,
        },
        {
          operation: "replace",
          governance_mode: "curator_route",
          requires_expected_revision: true,
        },
        {
          operation: "merge",
          governance_mode: "curator_route",
          requires_expected_revision: true,
        },
        {
          operation: "supersede",
          governance_mode: "curator_route",
          requires_expected_revision: true,
        },
        {
          operation: "archive",
          governance_mode: "manual_review",
          requires_expected_revision: true,
        },
      ],
    },
    conflict_policy: "supersession",
    operations: ["read", "list", "add", "replace", "merge", "supersede", "archive"],
    provenance_policy: {
      required_evidence: ["wake"],
      source_required: true,
      rationale_required: true,
    },
    retention_policy: "compact",
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
