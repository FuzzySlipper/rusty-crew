import type {
  ContextCompactionArtifact,
  ContextCompactionArtifactQuery,
  ManualContextCompactionRequest,
  ManualContextCompactionResponse,
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
  NativeProfileMemoryDelete,
  NativeProfileMemoryQuery,
  NativeProfileMemoryRecord,
  NativeProfileMemoryReplace,
  NativeProfileMemoryWrite,
  NativeSessionMemoryPromptContext,
  NativeSessionMemoryQuery,
  NativeSessionMemoryRecord,
} from "./public-api.js";

interface NativeBridgeMemoryBinding {
  listProfileMemory(query: NativeProfileMemoryQuery): unknown[];
  getProfileMemory(
    profileId: string,
    targetType: string,
    targetId: string | undefined,
    key: string,
  ): unknown;
  addProfileMemory(write: NativeProfileMemoryWrite): unknown;
  replaceProfileMemory(replace: NativeProfileMemoryReplace): unknown;
  removeProfileMemory(remove: NativeProfileMemoryDelete): unknown;
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
  manualContextCompactionJson(inputJson: string): string;
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
  | "manualContextCompaction"
  | "recordMemoryGovernanceDecision"
  | "listProfileMemory"
  | "getProfileMemory"
  | "addProfileMemory"
  | "replaceProfileMemory"
  | "removeProfileMemory"
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
    manualContextCompaction: async (request: ManualContextCompactionRequest) =>
      JSON.parse(
        binding.manualContextCompactionJson(JSON.stringify(request)),
      ) as ManualContextCompactionResponse,
    recordMemoryGovernanceDecision: async (
      decision: MemoryGovernanceDecisionInput,
    ) =>
      JSON.parse(
        binding.recordMemoryGovernanceDecisionJson(JSON.stringify(decision)),
      ) as MemoryGovernanceDecisionRecord,
    listProfileMemory: async (query) =>
      binding.listProfileMemory(query) as NativeProfileMemoryRecord[],
    getProfileMemory: async (input) =>
      (binding.getProfileMemory(
        input.profileId,
        input.targetType,
        input.targetId,
        input.key,
      ) as NativeProfileMemoryRecord | null) ?? undefined,
    addProfileMemory: async (write) =>
      binding.addProfileMemory(write) as NativeProfileMemoryRecord,
    replaceProfileMemory: async (replace) =>
      binding.replaceProfileMemory(replace) as NativeProfileMemoryRecord,
    removeProfileMemory: async (remove) =>
      binding.removeProfileMemory(remove) as NativeProfileMemoryRecord,
  };
}
