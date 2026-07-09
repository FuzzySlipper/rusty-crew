import type {
  ContextCompactionArtifact,
  ContextCompactionArtifactQuery,
  MemoryGovernanceDecisionInput,
  MemoryGovernanceDecisionRecord,
  MemoryProposalEnvelope,
  MemoryProposalQuery,
  MemoryProposalRecord,
  MemorySpaceDescriptor,
  SessionActivityDigest,
  SessionActivityDigestQuery,
} from "@rusty-crew/contracts";

import { validateBridgeValue } from "./bridge-validation.js";
import {
  rawContextCompactionArtifactArraySchema,
  rawContextCompactionArtifactQuerySchema,
  rawContextCompactionArtifactSchema,
  rawSessionActivityDigestArraySchema,
  rawSessionActivityDigestQuerySchema,
  rawSessionActivityDigestSchema,
} from "./bridge-validation-schemas.js";
import type {
  NativeBranchAwareSessionMemoryQuery,
  NativeBridgeModule,
  NativeSessionMemoryPromptContext,
  NativeSessionMemoryQuery,
  NativeSessionMemoryRecord,
} from "./index.js";

interface NativeBridgeMemoryBinding {
  listMemorySpaceDescriptorsJson(): string;
  querySessionMemoryRecordsJson(inputJson: string): string;
  buildSessionMemoryPromptContextJson(inputJson: string): string;
  saveMemoryProposalJson(inputJson: string): string;
  planCaptureMemoryProposalsJson(inputJson: string): string;
  planCuratorGovernanceTransitionJson(inputJson: string): string;
  planCuratorLifecycleTransitionJson(inputJson: string): string;
  planBackgroundMemoryAutoMutationsJson(inputJson: string): string;
  listMemoryProposalsJson(inputJson: string): string;
  saveSessionActivityDigestJson(inputJson: string): string;
  listSessionActivityDigestsJson(inputJson: string): string;
  saveContextCompactionArtifactJson(inputJson: string): string;
  listContextCompactionArtifactsJson(inputJson: string): string;
  recordMemoryGovernanceDecisionJson(inputJson: string): string;
}

type NativeBridgeMemoryMethods = Pick<
  NativeBridgeModule,
  | "listMemorySpaceDescriptors"
  | "querySessionMemoryRecords"
  | "buildSessionMemoryPromptContext"
  | "saveMemoryProposal"
  | "planCaptureMemoryProposals"
  | "planCuratorGovernanceTransition"
  | "planCuratorLifecycleTransition"
  | "planBackgroundMemoryAutoMutations"
  | "listMemoryProposals"
  | "saveSessionActivityDigest"
  | "listSessionActivityDigests"
  | "saveContextCompactionArtifact"
  | "listContextCompactionArtifacts"
  | "recordMemoryGovernanceDecision"
>;

export function createNativeBridgeMemoryMethods(
  binding: NativeBridgeMemoryBinding,
): NativeBridgeMemoryMethods {
  return {
    listMemorySpaceDescriptors: async () =>
      JSON.parse(
        binding.listMemorySpaceDescriptorsJson(),
      ) as MemorySpaceDescriptor[],
    querySessionMemoryRecords: async (query: NativeSessionMemoryQuery) =>
      JSON.parse(
        binding.querySessionMemoryRecordsJson(JSON.stringify(query)),
      ) as NativeSessionMemoryRecord[],
    buildSessionMemoryPromptContext: async (
      query: NativeBranchAwareSessionMemoryQuery,
    ) =>
      JSON.parse(
        binding.buildSessionMemoryPromptContextJson(JSON.stringify(query)),
      ) as NativeSessionMemoryPromptContext,
    saveMemoryProposal: async (proposal: MemoryProposalEnvelope) =>
      JSON.parse(
        binding.saveMemoryProposalJson(JSON.stringify(proposal)),
      ) as MemoryProposalRecord,
    planCaptureMemoryProposals: async (input: unknown) =>
      JSON.parse(
        binding.planCaptureMemoryProposalsJson(JSON.stringify(input)),
      ) as unknown,
    planCuratorGovernanceTransition: async (input: unknown) =>
      JSON.parse(
        binding.planCuratorGovernanceTransitionJson(JSON.stringify(input)),
      ) as unknown,
    planCuratorLifecycleTransition: async (input: unknown) =>
      JSON.parse(
        binding.planCuratorLifecycleTransitionJson(JSON.stringify(input)),
      ) as unknown,
    planBackgroundMemoryAutoMutations: async (input: unknown) =>
      JSON.parse(
        binding.planBackgroundMemoryAutoMutationsJson(JSON.stringify(input)),
      ) as unknown,
    listMemoryProposals: async (query: MemoryProposalQuery) =>
      JSON.parse(
        binding.listMemoryProposalsJson(JSON.stringify(query)),
      ) as MemoryProposalRecord[],
    saveSessionActivityDigest: async (digest: SessionActivityDigest) => {
      const validatedDigest = validateBridgeValue<SessionActivityDigest>({
        operation: "save_session_activity_digest",
        direction: "ts_to_rust",
        schema: rawSessionActivityDigestSchema,
        value: digest,
      });
      return validateBridgeValue<SessionActivityDigest>({
        operation: "save_session_activity_digest",
        direction: "rust_to_ts",
        schema: rawSessionActivityDigestSchema,
        value: JSON.parse(
          binding.saveSessionActivityDigestJson(
            JSON.stringify(validatedDigest),
          ),
        ),
      });
    },
    listSessionActivityDigests: async (query: SessionActivityDigestQuery) => {
      const validatedQuery = validateBridgeValue<SessionActivityDigestQuery>({
        operation: "list_session_activity_digests",
        direction: "ts_to_rust",
        schema: rawSessionActivityDigestQuerySchema,
        value: query,
      });
      return validateBridgeValue<SessionActivityDigest[]>({
        operation: "list_session_activity_digests",
        direction: "rust_to_ts",
        schema: rawSessionActivityDigestArraySchema,
        value: JSON.parse(
          binding.listSessionActivityDigestsJson(
            JSON.stringify(validatedQuery),
          ),
        ),
      });
    },
    saveContextCompactionArtifact: async (
      artifact: ContextCompactionArtifact,
    ) => {
      const validatedArtifact = validateBridgeValue<ContextCompactionArtifact>({
        operation: "save_context_compaction_artifact",
        direction: "ts_to_rust",
        schema: rawContextCompactionArtifactSchema,
        value: artifact,
      });
      return validateBridgeValue<ContextCompactionArtifact>({
        operation: "save_context_compaction_artifact",
        direction: "rust_to_ts",
        schema: rawContextCompactionArtifactSchema,
        value: JSON.parse(
          binding.saveContextCompactionArtifactJson(
            JSON.stringify(validatedArtifact),
          ),
        ),
      });
    },
    listContextCompactionArtifacts: async (
      query: ContextCompactionArtifactQuery,
    ) => {
      const validatedQuery =
        validateBridgeValue<ContextCompactionArtifactQuery>({
          operation: "list_context_compaction_artifacts",
          direction: "ts_to_rust",
          schema: rawContextCompactionArtifactQuerySchema,
          value: query,
        });
      return validateBridgeValue<ContextCompactionArtifact[]>({
        operation: "list_context_compaction_artifacts",
        direction: "rust_to_ts",
        schema: rawContextCompactionArtifactArraySchema,
        value: JSON.parse(
          binding.listContextCompactionArtifactsJson(
            JSON.stringify(validatedQuery),
          ),
        ),
      });
    },
    recordMemoryGovernanceDecision: async (
      decision: MemoryGovernanceDecisionInput,
    ) =>
      JSON.parse(
        binding.recordMemoryGovernanceDecisionJson(JSON.stringify(decision)),
      ) as MemoryGovernanceDecisionRecord,
  };
}
