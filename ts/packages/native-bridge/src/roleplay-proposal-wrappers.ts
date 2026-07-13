import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type RoleplayProposalMethodName =
  | "createRoleplayMechanicProposal"
  | "getRoleplayMechanicProposal"
  | "listRoleplayMechanicProposals"
  | "decideRoleplayMechanicProposal"
  | "applyRoleplayMechanicProposal";

export function createNativeBridgeRoleplayProposalMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, RoleplayProposalMethodName> {
  return {
    createRoleplayMechanicProposal: async (create) =>
      JSON.parse(
        binding.createRoleplayMechanicProposalJson(JSON.stringify(create)),
      ) as unknown,
    getRoleplayMechanicProposal: async (proposalId) =>
      (JSON.parse(binding.getRoleplayMechanicProposalJson(proposalId)) as
        | unknown
        | null) ?? undefined,
    listRoleplayMechanicProposals: async (query) =>
      JSON.parse(
        binding.listRoleplayMechanicProposalsJson(JSON.stringify(query)),
      ) as unknown[],
    decideRoleplayMechanicProposal: async (decision) =>
      JSON.parse(
        binding.decideRoleplayMechanicProposalJson(JSON.stringify(decision)),
      ) as unknown,
    applyRoleplayMechanicProposal: async (apply) =>
      JSON.parse(
        binding.applyRoleplayMechanicProposalJson(JSON.stringify(apply)),
      ) as unknown,
  };
}
