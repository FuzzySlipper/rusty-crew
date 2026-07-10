import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeBridgeModule,
  NativeLoreRecallResult,
  NativeLoreRecallTraceRecord,
  NativeRoleplayChatLayerRecord,
  NativeRoleplayLoreLayerConfigRecord,
  NativeRoleplayLoreLayerEntryJoin,
  NativeRoleplayLoreLayerRecord,
  NativeRoleplayLoreProvenanceEvent,
  NativeRoleplayLoreRecord,
} from "./public-api.js";

type RoleplayMethodName =
  | "putRoleplayCharacter"
  | "getRoleplayCharacter"
  | "listRoleplayCharacters"
  | "putRoleplayPlayerPersona"
  | "getRoleplayPlayerPersona"
  | "listRoleplayPlayerPersonas"
  | "putRoleplaySessionMetadata"
  | "getRoleplaySessionMetadata"
  | "listRoleplaySessionMetadata"
  | "applyRoleplaySessionProjection"
  | "putRoleplayImport"
  | "getRoleplayImport"
  | "listRoleplayImports"
  | "createLoreLayer"
  | "getLoreLayer"
  | "listLoreLayers"
  | "updateLoreLayer"
  | "archiveLoreLayer"
  | "setChatLayers"
  | "getChatLayers"
  | "toggleChatLayer"
  | "reorderChatLayers"
  | "addLoreEntry"
  | "replaceLoreEntry"
  | "supersedeLoreEntry"
  | "tombstoneLoreEntry"
  | "queryLoreEntries"
  | "getLoreEntry"
  | "loreEntryProvenanceEvents"
  | "addEntryToLayer"
  | "removeEntryFromLayer"
  | "setEntryConstant"
  | "listEntriesByLayer"
  | "recallLore"
  | "captureLoreFact"
  | "promoteLoreEntry"
  | "getLoreLayerConfig"
  | "setLoreLayerConfig"
  | "listRecallTraces"
  | "getRecallTrace"
  | "planRoleplayAssistantAlternative"
  | "planRoleplaySessionLifecycle"
  | "planRoleplayChatLayerBinding"
  | "normalizeRoleplayLoreSearchControls"
  | "readRoleplaySceneState"
  | "planRoleplaySceneStateUpdate"
  | "buildRoleplayPromptContext"
  | "roleplaySpeakerIdentity"
  | "writeRoleplayCharacter"
  | "mergeRoleplayCharacter"
  | "writeRoleplayPlayerPersona"
  | "mergeRoleplayPlayerPersona"
  | "patchRoleplaySessionMetadata"
  | "normalizeRoleplayNarratorConfig"
  | "startRoleplayNarratorTurn"
  | "advanceRoleplayNarratorTurn";

export function createNativeBridgeRoleplayMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, RoleplayMethodName> {
  return {
    putRoleplayCharacter: async (write) =>
      JSON.parse(
        binding.putRoleplayCharacterJson(JSON.stringify(write)),
      ) as unknown,
    getRoleplayCharacter: async (id) =>
      (JSON.parse(binding.getRoleplayCharacterJson(id)) as unknown | null) ??
      undefined,
    listRoleplayCharacters: async (query) =>
      JSON.parse(
        binding.listRoleplayCharactersJson(JSON.stringify(query)),
      ) as unknown[],
    putRoleplayPlayerPersona: async (write) =>
      JSON.parse(
        binding.putRoleplayPlayerPersonaJson(JSON.stringify(write)),
      ) as unknown,
    getRoleplayPlayerPersona: async (id) =>
      (JSON.parse(binding.getRoleplayPlayerPersonaJson(id)) as
        | unknown
        | null) ?? undefined,
    listRoleplayPlayerPersonas: async (query) =>
      JSON.parse(
        binding.listRoleplayPlayerPersonasJson(JSON.stringify(query)),
      ) as unknown[],
    putRoleplaySessionMetadata: async (write) =>
      JSON.parse(
        binding.putRoleplaySessionMetadataJson(JSON.stringify(write)),
      ) as unknown,
    getRoleplaySessionMetadata: async (id) =>
      (JSON.parse(binding.getRoleplaySessionMetadataJson(id)) as
        | unknown
        | null) ?? undefined,
    listRoleplaySessionMetadata: async (query) =>
      JSON.parse(
        binding.listRoleplaySessionMetadataJson(JSON.stringify(query)),
      ) as unknown[],
    applyRoleplaySessionProjection: async (write) =>
      JSON.parse(
        binding.applyRoleplaySessionProjectionJson(JSON.stringify(write)),
      ) as unknown,
    putRoleplayImport: async (write) =>
      JSON.parse(
        binding.putRoleplayImportJson(JSON.stringify(write)),
      ) as unknown,
    getRoleplayImport: async (id) =>
      (JSON.parse(binding.getRoleplayImportJson(id)) as unknown | null) ??
      undefined,
    listRoleplayImports: async (query) =>
      JSON.parse(
        binding.listRoleplayImportsJson(JSON.stringify(query)),
      ) as unknown[],
    createLoreLayer: async (write) =>
      JSON.parse(
        binding.createLoreLayerJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreLayerRecord,
    getLoreLayer: async (layerId) =>
      (JSON.parse(
        binding.getLoreLayerJson(layerId),
      ) as NativeRoleplayLoreLayerRecord | null) ?? undefined,
    listLoreLayers: async (profileId) =>
      JSON.parse(
        binding.listLoreLayersJson(profileId),
      ) as NativeRoleplayLoreLayerRecord[],
    updateLoreLayer: async (update) =>
      JSON.parse(
        binding.updateLoreLayerJson(JSON.stringify(update)),
      ) as NativeRoleplayLoreLayerRecord,
    archiveLoreLayer: async (archive) =>
      JSON.parse(
        binding.archiveLoreLayerJson(JSON.stringify(archive)),
      ) as NativeRoleplayLoreLayerRecord,
    setChatLayers: async (write) =>
      binding.setChatLayersJson(JSON.stringify(write)),
    getChatLayers: async (chatId) =>
      JSON.parse(
        binding.getChatLayersJson(chatId),
      ) as NativeRoleplayChatLayerRecord[],
    toggleChatLayer: async (input) =>
      binding.toggleChatLayerJson(
        JSON.stringify({
          chat_id: input.chatId,
          layer_id: input.layerId,
          enabled: input.enabled,
        }),
      ),
    reorderChatLayers: async (input) =>
      binding.reorderChatLayersJson(
        JSON.stringify({
          chat_id: input.chatId,
          layer_ids: input.layerIds,
        }),
      ),
    addLoreEntry: async (write) =>
      JSON.parse(
        binding.addLoreEntryJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreRecord,
    replaceLoreEntry: async (replace) =>
      JSON.parse(
        binding.replaceLoreEntryJson(JSON.stringify(replace)),
      ) as NativeRoleplayLoreRecord,
    supersedeLoreEntry: async (supersede) =>
      JSON.parse(binding.supersedeLoreEntryJson(JSON.stringify(supersede))) as [
        NativeRoleplayLoreRecord,
        NativeRoleplayLoreRecord,
      ],
    tombstoneLoreEntry: async (tombstone) =>
      JSON.parse(
        binding.tombstoneLoreEntryJson(JSON.stringify(tombstone)),
      ) as NativeRoleplayLoreRecord,
    queryLoreEntries: async (query) =>
      JSON.parse(
        binding.queryLoreEntriesJson(JSON.stringify(query)),
      ) as NativeRoleplayLoreRecord[],
    getLoreEntry: async (recordId) =>
      (JSON.parse(
        binding.getLoreEntryJson(recordId),
      ) as NativeRoleplayLoreRecord | null) ?? undefined,
    loreEntryProvenanceEvents: async (recordId) =>
      JSON.parse(
        binding.loreEntryProvenanceEventsJson(recordId),
      ) as NativeRoleplayLoreProvenanceEvent[],
    addEntryToLayer: async (link) =>
      binding.addEntryToLayerJson(JSON.stringify(link)),
    removeEntryFromLayer: async (input) =>
      binding.removeEntryFromLayerJson(
        JSON.stringify({
          layer_id: input.layerId,
          record_id: input.recordId,
        }),
      ),
    setEntryConstant: async (input) =>
      binding.setEntryConstantJson(
        JSON.stringify({
          layer_id: input.layerId,
          record_id: input.recordId,
          is_constant: input.isConstant,
        }),
      ),
    listEntriesByLayer: async (layerId) =>
      JSON.parse(
        binding.listEntriesByLayerJson(layerId),
      ) as NativeRoleplayLoreLayerEntryJoin[],
    recallLore: async (query) =>
      JSON.parse(
        binding.recallLoreJson(JSON.stringify(query)),
      ) as NativeLoreRecallResult,
    captureLoreFact: async (capture) =>
      JSON.parse(
        binding.captureLoreFactJson(JSON.stringify(capture)),
      ) as NativeRoleplayLoreLayerEntryJoin,
    promoteLoreEntry: async (promotion) =>
      JSON.parse(
        binding.promoteLoreEntryJson(JSON.stringify(promotion)),
      ) as NativeRoleplayLoreLayerEntryJoin,
    getLoreLayerConfig: async (layerId) =>
      (JSON.parse(
        binding.getLoreLayerConfigJson(layerId),
      ) as NativeRoleplayLoreLayerConfigRecord | null) ?? undefined,
    setLoreLayerConfig: async (write) =>
      JSON.parse(
        binding.setLoreLayerConfigJson(JSON.stringify(write)),
      ) as NativeRoleplayLoreLayerConfigRecord,
    listRecallTraces: async (query) =>
      JSON.parse(
        binding.listRecallTracesJson(JSON.stringify(query)),
      ) as NativeLoreRecallTraceRecord[],
    getRecallTrace: async (traceId) =>
      (JSON.parse(
        binding.getRecallTraceJson(traceId),
      ) as NativeLoreRecallTraceRecord | null) ?? undefined,
    planRoleplayAssistantAlternative: async (input) =>
      JSON.parse(
        binding.planRoleplayAssistantAlternativeJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplaySessionLifecycle: async (input) =>
      JSON.parse(
        binding.planRoleplaySessionLifecycleJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplayChatLayerBinding: async (input) =>
      JSON.parse(
        binding.planRoleplayChatLayerBindingJson(JSON.stringify(input)),
      ) as unknown,
    normalizeRoleplayLoreSearchControls: async (input) =>
      JSON.parse(
        binding.normalizeRoleplayLoreSearchControlsJson(JSON.stringify(input)),
      ) as unknown,
    readRoleplaySceneState: async (input) =>
      JSON.parse(
        binding.readRoleplaySceneStateJson(JSON.stringify(input)),
      ) as unknown,
    planRoleplaySceneStateUpdate: async (input) =>
      JSON.parse(
        binding.planRoleplaySceneStateUpdateJson(JSON.stringify(input)),
      ) as unknown,
    buildRoleplayPromptContext: async (input) =>
      JSON.parse(
        binding.buildRoleplayPromptContextJson(JSON.stringify(input)),
      ) as unknown,
    roleplaySpeakerIdentity: async (input) =>
      JSON.parse(
        binding.roleplaySpeakerIdentityJson(JSON.stringify(input)),
      ) as unknown,
    writeRoleplayCharacter: async (input) =>
      JSON.parse(
        binding.writeRoleplayCharacterJson(JSON.stringify(input)),
      ) as unknown,
    mergeRoleplayCharacter: async (input) =>
      JSON.parse(
        binding.mergeRoleplayCharacterJson(JSON.stringify(input)),
      ) as unknown,
    writeRoleplayPlayerPersona: async (input) =>
      JSON.parse(
        binding.writeRoleplayPlayerPersonaJson(JSON.stringify(input)),
      ) as unknown,
    mergeRoleplayPlayerPersona: async (input) =>
      JSON.parse(
        binding.mergeRoleplayPlayerPersonaJson(JSON.stringify(input)),
      ) as unknown,
    patchRoleplaySessionMetadata: async (input) =>
      JSON.parse(
        binding.patchRoleplaySessionMetadataJson(JSON.stringify(input)),
      ) as unknown,
    normalizeRoleplayNarratorConfig: async (input) =>
      JSON.parse(
        binding.normalizeRoleplayNarratorConfigJson(JSON.stringify(input)),
      ) as unknown,
    startRoleplayNarratorTurn: async (input) =>
      JSON.parse(
        binding.startRoleplayNarratorTurnJson(JSON.stringify(input)),
      ) as unknown,
    advanceRoleplayNarratorTurn: async (input) =>
      JSON.parse(
        binding.advanceRoleplayNarratorTurnJson(JSON.stringify(input)),
      ) as unknown,
  };
}
