import assert from "node:assert/strict";
import {
  assertValidMemoryProposalEnvelope,
  assertValidMemorySpaceDescriptor,
  type MemoryOperation,
  type MemoryProposalEnvelope,
  type MemoryRecordShapeId,
  type MemorySpaceId,
  type MemorySpaceDescriptor,
} from "./index.js";

const profileDense = memorySpace("profile_dense", {
  moduleId: "runtime_memory",
  scopes: ["profile", "user"],
  primaryScope: "profile",
  shapeId: "profile_dense_item",
  operations: ["read", "list", "add", "replace", "remove"],
});
const sessionMemory = memorySpace("session_memory", {
  moduleId: "runtime_memory",
  scopes: ["session", "conversation_branch"],
  primaryScope: "session",
  shapeId: "session_fact",
  operations: ["read", "list", "add", "merge", "supersede"],
});
const roleplayLore = memorySpace("roleplay_lore", {
  moduleId: "roleplay_lore",
  scopes: ["world", "entity", "session", "conversation_branch"],
  primaryScope: "world",
  shapeId: "lore_entry",
  operations: ["read", "list", "add", "merge", "supersede", "archive"],
});

for (const descriptor of [profileDense, sessionMemory, roleplayLore]) {
  assertValidMemorySpaceDescriptor(descriptor);
  const roundTrip = JSON.parse(
    JSON.stringify(descriptor),
  ) as MemorySpaceDescriptor;
  assert.deepEqual(roundTrip, descriptor);
}

const proposal: MemoryProposalEnvelope = {
  proposal_id: "proposal_one",
  space_id: memorySpaceId("profile_dense"),
  operation: "add",
  scope: { scope_type: "profile", scope_id: "rusty-crew-runner" },
  shape: { shape_id: memoryShapeId("profile_dense_item"), version: 1 },
  content: {
    key: "memory_boundary",
    content: "Use Crew profile memory for stable local preferences.",
  },
  evidence_refs: [{ evidence_type: "wake", ref_id: "wake-1" }],
  confidence: 0.82,
  durability_rationale: "Stable profile-local preference.",
  governance_mode: "candidate",
  source: "in_wake_tool",
  dedupe_key: "profile_dense:memory_boundary",
  created_at: "2026-06-26T00:00:00Z",
};

assertValidMemoryProposalEnvelope(proposal, profileDense);

const proposalRoundTrip = JSON.parse(
  JSON.stringify(proposal),
) as MemoryProposalEnvelope;
assert.deepEqual(proposalRoundTrip, proposal);

assert.throws(() =>
  assertValidMemorySpaceDescriptor({
    ...profileDense,
    space_id: memorySpaceId("ProfileDense"),
  }),
);

assert.throws(() =>
  assertValidMemoryProposalEnvelope(
    {
      ...proposal,
      operation: "read",
    },
    profileDense,
  ),
);

assert.throws(() =>
  assertValidMemoryProposalEnvelope(
    {
      ...proposal,
      scope: { scope_type: "world", scope_id: "world-1" },
    },
    profileDense,
  ),
);

console.log("memory-space contract smoke passed");

function memorySpace(
  spaceId: string,
  options: {
    moduleId: string;
    scopes: MemorySpaceDescriptor["scope_model"]["allowed_scopes"];
    primaryScope: MemorySpaceDescriptor["scope_model"]["primary_scope"];
    shapeId: string;
    operations: MemoryOperation[];
  },
): MemorySpaceDescriptor {
  return {
    space_id: memorySpaceId(spaceId),
    schema_version: 1,
    module_id: options.moduleId,
    description: `${spaceId} Crew memory descriptor`,
    record_shapes: [
      {
        shape_id: memoryShapeId(options.shapeId),
        version: 1,
        description: `${options.shapeId} shape`,
        fields: [
          {
            field_name: "content",
            field_type: "markdown",
            required: true,
            description: "Durable content",
          },
        ],
      },
    ],
    scope_model: {
      allowed_scopes: options.scopes,
      primary_scope: options.primaryScope,
    },
    visibility_model:
      options.primaryScope === "world" ? "world_scoped" : "profile_local",
    retrieval_strategies: ["direct_lookup", "query_search"],
    indexing: {
      required_capabilities: ["key_lookup"],
      optional_capabilities: ["text_search"],
    },
    prompt_policy:
      options.primaryScope === "world"
        ? "explicit_user_context"
        : "summary_context",
    write_policy: {
      default_mode: "candidate",
      operation_policies: options.operations
        .filter((operation) => !["read", "list"].includes(operation))
        .map((operation) => ({
          operation,
          governance_mode:
            options.primaryScope === "world" ? "manual_review" : "candidate",
          requires_expected_revision: ["replace", "remove"].includes(operation),
          min_confidence: 0.5,
        })),
    },
    operations: options.operations,
    provenance_policy: {
      required_evidence: ["wake"],
      source_required: true,
      rationale_required: true,
    },
    retention_policy:
      options.primaryScope === "world" ? "domain_specific" : "manual_only",
    conflict_policy:
      options.primaryScope === "world"
        ? "domain_specific"
        : "expected_revision",
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

function memorySpaceId(value: string): MemorySpaceId {
  return value as MemorySpaceId;
}

function memoryShapeId(value: string): MemoryRecordShapeId {
  return value as MemoryRecordShapeId;
}
