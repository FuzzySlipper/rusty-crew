import type { NativeBridgeModule } from "./public-api.js";

interface NativeBridgeCuratorBinding {
  applyCuratorGovernanceWriteJson(inputJson: string): string;
  getCuratorCandidateJson(candidateId: string): string;
  listCuratorCandidatesJson(inputJson: string): string;
  getCuratorMutationJson(mutationId: string): string;
  listCuratorMutationsJson(inputJson: string): string;
  listCuratorAuditReceiptsJson(inputJson: string): string;
}

type NativeBridgeCuratorMethods = Pick<
  NativeBridgeModule,
  | "applyCuratorGovernanceWrite"
  | "getCuratorCandidate"
  | "listCuratorCandidates"
  | "getCuratorMutation"
  | "listCuratorMutations"
  | "listCuratorAuditReceipts"
>;

export function createNativeBridgeCuratorMethods(
  binding: NativeBridgeCuratorBinding,
): NativeBridgeCuratorMethods {
  return {
    applyCuratorGovernanceWrite: async (input: unknown) =>
      JSON.parse(
        binding.applyCuratorGovernanceWriteJson(JSON.stringify(input)),
      ) as unknown,
    getCuratorCandidate: async (candidateId: string) =>
      JSON.parse(binding.getCuratorCandidateJson(candidateId)) as
        | unknown
        | undefined,
    listCuratorCandidates: async (query: unknown) =>
      JSON.parse(
        binding.listCuratorCandidatesJson(JSON.stringify(query)),
      ) as unknown,
    getCuratorMutation: async (mutationId: string) =>
      JSON.parse(binding.getCuratorMutationJson(mutationId)) as
        | unknown
        | undefined,
    listCuratorMutations: async (query: unknown) =>
      JSON.parse(
        binding.listCuratorMutationsJson(JSON.stringify(query)),
      ) as unknown,
    listCuratorAuditReceipts: async (query: unknown) =>
      JSON.parse(
        binding.listCuratorAuditReceiptsJson(JSON.stringify(query)),
      ) as unknown,
  };
}
