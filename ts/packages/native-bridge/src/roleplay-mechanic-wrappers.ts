import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type { NativeBridgeModule } from "./public-api.js";

type RoleplayMechanicMethodName =
  | "createRoleplayMechanicSessionAssociation"
  | "getRoleplayMechanicSessionAssociation"
  | "listRoleplayMechanicSessionAssociations"
  | "updateRoleplayMechanicSessionAttachment"
  | "createRoleplayMechanicDiagnostic"
  | "getRoleplayMechanicDiagnostic"
  | "listRoleplayMechanicDiagnostics"
  | "updateRoleplayMechanicDiagnosticOutcome";

export function createNativeBridgeRoleplayMechanicMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, RoleplayMechanicMethodName> {
  const parse = (value: string): unknown => JSON.parse(value) as unknown;
  return {
    createRoleplayMechanicSessionAssociation: async (create) =>
      parse(
        binding.createRoleplayMechanicSessionAssociationJson(
          JSON.stringify(create),
        ),
      ),
    getRoleplayMechanicSessionAssociation: async (sessionId) =>
      parse(binding.getRoleplayMechanicSessionAssociationJson(sessionId)) ??
      undefined,
    listRoleplayMechanicSessionAssociations: async (query) =>
      parse(
        binding.listRoleplayMechanicSessionAssociationsJson(
          JSON.stringify(query),
        ),
      ) as unknown[],
    updateRoleplayMechanicSessionAttachment: async (update) =>
      parse(
        binding.updateRoleplayMechanicSessionAttachmentJson(
          JSON.stringify(update),
        ),
      ),
    createRoleplayMechanicDiagnostic: async (create) =>
      parse(
        binding.createRoleplayMechanicDiagnosticJson(JSON.stringify(create)),
      ),
    getRoleplayMechanicDiagnostic: async (diagnosticId) =>
      parse(binding.getRoleplayMechanicDiagnosticJson(diagnosticId)) ??
      undefined,
    listRoleplayMechanicDiagnostics: async (query) =>
      parse(
        binding.listRoleplayMechanicDiagnosticsJson(JSON.stringify(query)),
      ) as unknown[],
    updateRoleplayMechanicDiagnosticOutcome: async (update) =>
      parse(
        binding.updateRoleplayMechanicDiagnosticOutcomeJson(
          JSON.stringify(update),
        ),
      ),
  };
}
