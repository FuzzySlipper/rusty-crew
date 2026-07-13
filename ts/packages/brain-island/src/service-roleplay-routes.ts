import type { IncomingMessage } from "node:http";
import { randomBytes } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type { ProfileId, SessionId, SessionState } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { failure, successRoute } from "./service-route-results.js";
import { loadProfileConfig } from "./profile-loading.js";
import { createLocalToolProfileStore } from "./local-tool-profiles.js";
import {
  handleAdminRoleplayLoreRequest,
  handleBrowserProfileLoreLayersRequest,
} from "./roleplay/lore-routes.js";
import type {
  ChatActor,
  ChatEvent,
  ConversationBranchRecord,
  MessageSlotRecord,
  MessageVariantRecord,
} from "./rusty-view-chat-api.js";

export interface RoleplayRouteContext {
  readonly bridge: NativeBridgeModule;
  readonly runtimeConfig: { readonly profilesDir: string };
  readonly now: () => string;
  applyServiceRuntimeConfigFromDisk(options: {
    createMissingSessions: boolean;
    eventType: string;
    summaryPrefix: string;
  }): Promise<unknown>;
  rebuildBrainRuntime(profileId: ProfileId): Promise<void>;
  serviceSessionById(sessionId: string): Promise<SessionState>;
  listChatEventsAfterCursor(
    session: SessionState,
    afterCursor: string | undefined,
    limit: number,
  ): Promise<readonly ChatEvent[]>;
  generateRoleplayAssistantAlternative?(
    input: RoleplayAssistantAlternativeGenerationInput,
  ): Promise<RoleplayAssistantAlternativeGenerationResult>;
}

export interface RoleplayAssistantAlternativeGenerationInput {
  session: SessionState;
  slot: MessageSlotRecord;
  prompt: string;
  requestId: string;
}

export interface RoleplayAssistantAlternativeGenerationResult {
  body: string;
  wakeId?: string;
  summary?: string;
  metadataJson?: Record<string, unknown>;
}

export function isRoleplayBrowserRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/v1/admin/roleplay/") ||
    /^\/v1\/profile\/[^/]+\/layers\/?$/.test(pathname)
  );
}

interface RoleplayCharacterRecord {
  id: string;
  profileId: string;
  name: string;
  description: string;
  personality: string;
  scenario: string;
  firstMessage: string;
  alternateGreetings: string[];
  exampleMessages: string[];
  tags: string[];
  avatarUrl?: string;
  status: "active" | "archived";
  revision: number;
  createdAt: string;
  updatedAt: string;
}

interface RoleplayPlayerPersonaRecord {
  id: string;
  profileId: string;
  displayName: string;
  avatarUrl?: string;
  avatarAssetRef?: string;
  description: string;
  notes: string;
  status: "active" | "archived";
  revision: number;
  createdAt: string;
  updatedAt: string;
}

interface RoleplaySessionMetadata {
  sessionId: string;
  profileId: string;
  displayName?: string;
  playerPersonaId?: string;
  characterId?: string;
  activeLayerIds: string[];
  archived: boolean;
  narratorDiagnostic?: {
    wakeId: string;
    sceneBrief: string;
    relevantLoreRecordIds: string[];
    updatedAt: string;
  };
  revision: number;
  createdAt: string;
  updatedAt: string;
}

interface RoleplaySessionMetadataPatchOutput {
  metadata: RoleplaySessionMetadata;
  active_layer_ids_changed: boolean;
}

interface RoleplaySessionAlternativeSlot {
  slot_id: string;
  active_variant_id?: string | null;
  primary_variant_id: string;
  alternate_count: number;
  variant_count: number;
  active_variant?: MessageVariantRecord;
  variants: MessageVariantRecord[];
}

interface RoleplayAssistantAlternativePlan {
  session_id: string;
  terminal_slot: MessageSlotRecord;
  active_variant: MessageVariantRecord;
  variant_projection: RoleplaySessionAlternativeSlot;
  next_alternate_ordinal: number;
  branch_id_for_variant?: string | null;
  parent_message_id?: string | null;
  previous_message_id?: string | null;
  branch_head_update?: { branch_id: string; head_message_id: string } | null;
  append_chat_message: boolean;
  variant_write?: {
    slot_id: string;
    variant_id: string;
    message_id: string;
    source: "alternate" | string;
    ordinal: number;
    branch_id?: string | null;
    parent_message_id?: string | null;
    previous_message_id?: string | null;
  };
}

interface RoleplaySessionLifecycleSession {
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: string;
  status: string;
  created_at: string;
  updated_at: string;
}

interface RoleplayChatLayerBinding {
  layer_id: string;
  priority: number;
  enabled: boolean;
}

interface RoleplayChatLayerUpdatePlan {
  chat_id: string;
  layers: RoleplayChatLayerBinding[];
}

interface RoleplayRuntimeSessionPlan {
  create_session: boolean;
  archive_session: boolean;
  ensure_configured_session: boolean;
}

interface RoleplaySessionForkPlan {
  source_session_id: string;
  source_message_id: string;
  target_session_id: string;
  branch_id: string;
  branch_label: string;
  branch_metadata_json: unknown;
}

interface RoleplaySessionLifecyclePlan {
  action: string;
  session_id: string;
  agent_id: string;
  profile_id: string;
  kind: string;
  metadata: RoleplaySessionMetadata;
  runtime: RoleplayRuntimeSessionPlan;
  chat_layer_update?: RoleplayChatLayerUpdatePlan;
  fork?: RoleplaySessionForkPlan;
}

export interface RoleplaySpeakerIdentitySnapshot {
  speaker_kind:
    | "player_persona"
    | "fallback_player"
    | "assistant_character"
    | "fallback_assistant"
    | "system";
  role: "user" | "assistant" | "system";
  source_id?: string;
  display_name: string;
  avatar_url?: string;
  avatar_asset_ref?: string;
  snapshot_at: string;
}

type RoleplayNarratorTone =
  | "whimsical"
  | "dramatic"
  | "matter_of_fact"
  | "lush"
  | "wry";

type RoleplayNarratorPacing = "leisurely" | "balanced" | "rapid" | "breathless";

type RoleplayNarratorExplicitness =
  | "implied"
  | "suggestive"
  | "romantic"
  | "steamy";

type RoleplayNarratorMemoryDepth = "shallow" | "medium" | "deep";

interface BrowserRoleplayNarratorConfig {
  tone: RoleplayNarratorTone;
  pacing: RoleplayNarratorPacing;
  explicitness: RoleplayNarratorExplicitness;
  memoryDepth: RoleplayNarratorMemoryDepth;
  stylePrompt?: string;
  exemplar?: string;
  review: {
    enabled: boolean;
    maxReviewCycles: number;
  };
}

interface BrowserRoleplayMechanicProfilePlan {
  config: {
    name: string;
    providerAlias?: string;
    autoMonitor: {
      enabled: false;
      available: false;
      status: "inactive_future";
    };
  };
  systemPrompt: string;
  localToolProfileId: "roleplay_mechanic";
}

interface RoleplayPromptContextOutput {
  prompt_context?: string;
  stack?: Record<string, unknown>;
}

export async function handleAdminRoleplayRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const profileLayersMatch = url.pathname.match(
    /^\/v1\/profile\/([^/]+)\/layers\/?$/,
  );
  if (profileLayersMatch) {
    return handleBrowserProfileLoreLayersRequest(
      request,
      state,
      url,
      decodeURIComponent(profileLayersMatch[1]),
    );
  }
  const parts = url.pathname.split("/").filter(Boolean);
  if (
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "roleplay" &&
    parts[3] === "imports" &&
    parts[4] === "st-packet"
  ) {
    return handleRoleplayStPacketImportRequest(request, state);
  }
  if (
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "roleplay" &&
    parts[3] === "profiles" &&
    parts[4]
  ) {
    const profileId = decodeURIComponent(parts[4]);
    if (parts[5] === "characters") {
      return handleRoleplayCharacterRequest(request, state, url, profileId);
    }
    if (parts[5] === "personas") {
      return handleRoleplayPlayerPersonaRequest(request, state, url, profileId);
    }
    if (parts[5] === "narrator-config") {
      return handleRoleplayNarratorConfigRequest(
        request,
        state,
        url,
        profileId,
      );
    }
    if (parts[5] === "mechanic-config") {
      return handleRoleplayMechanicConfigRequest(
        request,
        state,
        url,
        profileId,
      );
    }
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/sessions")) {
    return handleRoleplaySessionRequest(request, state, url);
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/lore/")) {
    return handleAdminRoleplayLoreRequest(request, state, url, {
      sessionMetadata: async (sessionId) => {
        const session = await state.serviceSessionById(sessionId);
        return roleplaySessionMetadata(state, session);
      },
      upsertSessionMetadata: (sessionId, patch) =>
        upsertRoleplaySessionMetadata(state, sessionId, patch),
    });
  }
  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: "unknown_roleplay_admin_route",
    message: `unknown roleplay route ${url.pathname}`,
    retryable: false,
  });
}

async function handleRoleplayCharacterRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const characterId =
    parts.length >= 7 ? decodeURIComponent(parts[6]) : undefined;
  try {
    if (characterId === undefined) {
      if (method === "GET") {
        const includeArchived =
          url.searchParams.get("include_archived") === "true";
        const characters = (
          await listRoleplayCharacters(state, profileId)
        ).filter(
          (character) => includeArchived || character.status !== "archived",
        );
        return successRoute(requestIdValue, {
          profileId,
          items: characters,
          total: characters.length,
        });
      }
      if (method === "POST") {
        const character = (await state.bridge.writeRoleplayCharacter({
          profile_id: profileId,
          now: state.now(),
          fallback_id: `character-${randomBytes(6).toString("hex")}`,
          body: recordBody(await readJsonBody(request)),
        })) as RoleplayCharacterRecord;
        const stored = await putRoleplayCharacter(state, character);
        return successRoute(requestIdValue, { character: stored });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay character collection supports GET and POST",
      );
    }

    if (method === "GET") {
      const character = await getRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      if (character === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_character_not_found",
          `roleplay character ${characterId} was not found`,
        );
      }
      return successRoute(requestIdValue, { character });
    }
    if (method === "PATCH") {
      const current = await requireRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      const character = (await state.bridge.mergeRoleplayCharacter({
        current,
        body: recordBody(await readJsonBody(request)),
        now: state.now(),
      })) as RoleplayCharacterRecord;
      const stored = await putRoleplayCharacter(
        state,
        character,
        current.revision,
      );
      return successRoute(requestIdValue, { character: stored });
    }
    if (method === "DELETE") {
      const current = await requireRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      const character = (await state.bridge.mergeRoleplayCharacter({
        current,
        body: { status: "archived" },
        now: state.now(),
      })) as RoleplayCharacterRecord;
      const stored = await putRoleplayCharacter(
        state,
        character,
        current.revision,
      );
      return successRoute(requestIdValue, { character: stored });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay character item supports GET, PATCH, and DELETE",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_character_request_failed",
      error,
    );
  }
}

async function handleRoleplayPlayerPersonaRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const personaId =
    parts.length >= 7 ? decodeURIComponent(parts[6]) : undefined;
  try {
    if (personaId === undefined) {
      if (method === "GET") {
        const includeArchived =
          url.searchParams.get("include_archived") === "true";
        const personas = (
          await listRoleplayPlayerPersonas(state, profileId)
        ).filter((persona) => includeArchived || persona.status !== "archived");
        return successRoute(requestIdValue, {
          profileId,
          items: personas,
          total: personas.length,
        });
      }
      if (method === "POST") {
        const persona = (await state.bridge.writeRoleplayPlayerPersona({
          profile_id: profileId,
          now: state.now(),
          fallback_id: `persona-${randomBytes(6).toString("hex")}`,
          body: recordBody(await readJsonBody(request)),
        })) as RoleplayPlayerPersonaRecord;
        const stored = await putRoleplayPlayerPersona(state, persona);
        return successRoute(requestIdValue, { persona: stored });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay player persona collection supports GET and POST",
      );
    }

    if (method === "GET") {
      const persona = await getRoleplayPlayerPersona(
        state,
        profileId,
        personaId,
      );
      if (persona === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_player_persona_not_found",
          `roleplay player persona ${personaId} was not found`,
        );
      }
      return successRoute(requestIdValue, { persona });
    }
    if (method === "PATCH") {
      const current = await requireRoleplayPlayerPersona(
        state,
        profileId,
        personaId,
      );
      const persona = (await state.bridge.mergeRoleplayPlayerPersona({
        current,
        body: recordBody(await readJsonBody(request)),
        now: state.now(),
      })) as RoleplayPlayerPersonaRecord;
      const stored = await putRoleplayPlayerPersona(
        state,
        persona,
        current.revision,
      );
      return successRoute(requestIdValue, { persona: stored });
    }
    if (method === "DELETE") {
      const current = await requireRoleplayPlayerPersona(
        state,
        profileId,
        personaId,
      );
      const persona = (await state.bridge.mergeRoleplayPlayerPersona({
        current,
        body: { status: "archived" },
        now: state.now(),
      })) as RoleplayPlayerPersonaRecord;
      const stored = await putRoleplayPlayerPersona(
        state,
        persona,
        current.revision,
      );
      return successRoute(requestIdValue, { persona: stored });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay player persona item supports GET, PATCH, and DELETE",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_player_persona_request_failed",
      error,
    );
  }
}

async function handleRoleplaySessionRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  url: URL,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean);
  const sessionId =
    parts.length >= 5 ? decodeURIComponent(parts[4]) : undefined;
  const action = parts.length >= 6 ? parts[5] : undefined;
  const childId = parts.length >= 7 ? decodeURIComponent(parts[6]) : undefined;
  const childAction = parts.length >= 8 ? parts[7] : undefined;
  try {
    if (sessionId === undefined) {
      if (method === "GET") {
        const profileId = url.searchParams.get("profile_id") ?? undefined;
        return successRoute(requestIdValue, {
          items: await listRoleplaySessions(state, profileId),
        });
      }
      if (method === "POST") {
        return successRoute(requestIdValue, {
          session: await createRoleplaySession(
            state,
            recordBody(await readJsonBody(request)),
          ),
        });
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay session collection supports GET and POST",
      );
    }

    if (action === "archive" && method === "POST") {
      const archived = await archiveRoleplaySession(state, sessionId);
      return successRoute(requestIdValue, { session: archived });
    }
    if (action === "restore" && method === "POST") {
      const restored = await restoreRoleplaySession(state, sessionId);
      return successRoute(requestIdValue, { session: restored });
    }
    if (
      action === "prompt-stack" &&
      childId === undefined &&
      childAction === undefined
    ) {
      if (method !== "GET") {
        return roleplayLoreMethodNotAllowed(
          requestIdValue,
          "roleplay session prompt stack supports GET",
        );
      }
      const session = (await state.bridge.listSessions()).find(
        (candidate) => candidate.sessionId === sessionId,
      );
      if (session === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_session_not_found",
          `roleplay session ${sessionId} was not found`,
        );
      }
      const output = await roleplayPromptContextOutputForSession(
        state,
        session,
      );
      return successRoute(requestIdValue, {
        sessionId,
        profileId: session.profileId,
        promptContext: output?.prompt_context,
        stack: output?.stack,
      });
    }
    if (action === "fork" && method === "POST") {
      const fork = await forkRoleplaySessionAtMessage(
        state,
        sessionId,
        recordBody(await readJsonBody(request)),
      );
      return roleplaySuccess(requestIdValue, fork, 201);
    }
    if (action === "alternatives" && childId === undefined) {
      if (method === "GET") {
        return successRoute(
          requestIdValue,
          await roleplayTerminalAlternativesResult(state, sessionId, url),
        );
      }
      if (method === "POST") {
        const alternative = await createRoleplayAssistantAlternative(
          state,
          sessionId,
          recordBody(await readJsonBody(request)),
          requestIdValue,
        );
        return roleplaySuccess(requestIdValue, alternative, 201);
      }
      return roleplayLoreMethodNotAllowed(
        requestIdValue,
        "roleplay session alternatives supports GET and POST",
      );
    }
    if (
      action === "alternatives" &&
      childId === "generate" &&
      childAction === undefined &&
      method === "POST"
    ) {
      const alternative = await generateRoleplayAssistantAlternative(
        state,
        sessionId,
        recordBody(await readJsonBody(request)),
        requestIdValue,
      );
      return roleplaySuccess(requestIdValue, alternative, 201);
    }
    if (
      action === "alternatives" &&
      childId !== undefined &&
      childAction === "select" &&
      method === "POST"
    ) {
      const selected = await selectRoleplayAssistantAlternative(
        state,
        sessionId,
        childId,
        recordBody(await readJsonBody(request)),
      );
      return roleplaySuccess(
        requestIdValue,
        selected,
        selected.status === "conflict" ? 409 : 200,
      );
    }
    if (action !== undefined) {
      return roleplayNotFound(
        requestIdValue,
        "unknown_roleplay_session_action",
        `unknown roleplay session action ${action}`,
      );
    }
    if (method === "GET") {
      const session = await getRoleplaySessionSummary(state, sessionId);
      if (session === undefined) {
        return roleplayNotFound(
          requestIdValue,
          "roleplay_session_not_found",
          `roleplay session ${sessionId} was not found`,
        );
      }
      return successRoute(requestIdValue, { session });
    }
    if (method === "PATCH") {
      const session = await updateRoleplaySessionMetadata(
        state,
        sessionId,
        recordBody(await readJsonBody(request)),
      );
      return successRoute(requestIdValue, { session });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay session item supports GET and PATCH",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_session_request_failed",
      error,
    );
  }
}

async function handleRoleplayStPacketImportRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  if (method !== "POST") {
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay ST packet import supports POST",
    );
  }
  try {
    return successRoute(
      requestIdValue,
      await importRoleplayStPacket(
        state,
        recordBody(await readJsonBody(request)),
      ),
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_st_packet_import_failed",
      error,
    );
  }
}

async function handleRoleplayNarratorConfigRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  _url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  try {
    if (method === "GET") {
      return successRoute(requestIdValue, {
        profileId,
        config: await readRoleplayNarratorConfig(state, profileId),
        applies: "next_wake",
      });
    }
    if (method === "PATCH" || method === "POST") {
      const config = await writeRoleplayNarratorConfig(
        state,
        profileId,
        recordBody(await readJsonBody(request)),
      );
      return successRoute(requestIdValue, {
        profileId,
        config,
        applies: "next_wake",
      });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay narrator config supports GET, PATCH, and POST",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_narrator_config_request_failed",
      error,
    );
  }
}

async function handleRoleplayMechanicConfigRequest(
  request: IncomingMessage,
  state: RoleplayRouteContext,
  _url: URL,
  profileId: string,
): Promise<AdminRouteResult> {
  const requestIdValue = requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  try {
    if (method === "GET") {
      const result = await readRoleplayMechanicConfig(state, profileId);
      return successRoute(requestIdValue, {
        profileId,
        ...result,
        applies: "next_wake",
      });
    }
    if (method === "PATCH" || method === "POST") {
      const result = await writeRoleplayMechanicConfig(
        state,
        profileId,
        recordBody(await readJsonBody(request)),
      );
      return successRoute(requestIdValue, {
        profileId,
        ...result,
        applies: "next_wake",
      });
    }
    return roleplayLoreMethodNotAllowed(
      requestIdValue,
      "roleplay mechanic config supports GET, PATCH, and POST",
    );
  } catch (error) {
    return roleplayInputError(
      requestIdValue,
      "roleplay_mechanic_config_request_failed",
      error,
    );
  }
}

async function importRoleplayStPacket(
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const now = state.now();
  const profileId = requiredString(
    body.profileId ?? body.profile_id,
    "profileId",
  );
  const importId =
    optionalString(body.importId) ??
    optionalString(body.import_id) ??
    stableRoleplayRecordId("st-import", `${profileId}:${now}`);
  const provenance = optionalRecord(body.provenance) ?? {};
  const rawSource =
    optionalRecord(body.rawSource) ?? optionalRecord(body.raw_source);
  const character = await importRoleplayCharacter(state, {
    profileId,
    importId,
    now,
    provenance,
    body: optionalRecord(body.character),
    rawSource,
  });
  const persona = await importRoleplayPersona(state, {
    profileId,
    importId,
    now,
    provenance,
    body: optionalRecord(body.persona),
    rawSource,
  });
  const lore = await importRoleplayLore(state, {
    profileId,
    importId,
    now,
    provenance,
    layer: optionalRecord(body.loreLayer) ?? optionalRecord(body.lore_layer),
    entries: arrayValue(body.loreEntries ?? body.lore_entries),
  });
  const session = await importRoleplayTranscript(state, {
    profileId,
    importId,
    now,
    provenance,
    characterId: character?.id,
    personaId: persona?.id,
    activeLayerIds: lore.layerId === undefined ? [] : [lore.layerId],
    session: optionalRecord(body.session),
    rows: arrayValue(
      body.transcriptRows ?? body.transcript_rows ?? body.messages,
    ),
  });
  await state.bridge.putRoleplayImport({
    record: {
      importId,
      profileId,
      sourceKind: "sillytavern_packet",
      provenance,
      rawSource,
      importedAt: now,
      updatedAt: now,
      characterId: character?.id,
      personaId: persona?.id,
      loreLayerId: lore.layerId,
      sessionId: session.sessionId,
      counts: {
        characters: character === undefined ? 0 : 1,
        personas: persona === undefined ? 0 : 1,
        loreEntries: lore.entryCount,
        messages: session.messageCount,
        assistantVariantRows: session.assistantVariantRows,
        assistantMultiSwipeRows: session.assistantMultiSwipeRows,
        variants: session.variantCount,
      },
      status: "completed",
      revision: 0,
    },
  });
  return {
    importId,
    profileId,
    provenance,
    character,
    persona,
    lore,
    session,
    counts: {
      characters: character === undefined ? 0 : 1,
      personas: persona === undefined ? 0 : 1,
      loreEntries: lore.entryCount,
      messages: session.messageCount,
      assistantVariantRows: session.assistantVariantRows,
      assistantMultiSwipeRows: session.assistantMultiSwipeRows,
      variants: session.variantCount,
    },
  };
}

async function putRoleplayCharacter(
  state: RoleplayRouteContext,
  record: RoleplayCharacterRecord,
  expectedRevision?: number,
): Promise<RoleplayCharacterRecord> {
  return state.bridge.putRoleplayCharacter({
    record: { ...record, revision: expectedRevision ?? 0 },
    ...(expectedRevision === undefined
      ? {}
      : { expected_revision: expectedRevision }),
  }) as Promise<RoleplayCharacterRecord>;
}

async function putRoleplayPlayerPersona(
  state: RoleplayRouteContext,
  record: RoleplayPlayerPersonaRecord,
  expectedRevision?: number,
): Promise<RoleplayPlayerPersonaRecord> {
  return state.bridge.putRoleplayPlayerPersona({
    record: { ...record, revision: expectedRevision ?? 0 },
    ...(expectedRevision === undefined
      ? {}
      : { expected_revision: expectedRevision }),
  }) as Promise<RoleplayPlayerPersonaRecord>;
}

async function putRoleplaySessionMetadataRecord(
  state: RoleplayRouteContext,
  record: RoleplaySessionMetadata,
  expectedRevision?: number,
  chatLayers?: { chat_id: string; layers: unknown[]; now: string },
): Promise<RoleplaySessionMetadata> {
  const projection = (await state.bridge.applyRoleplaySessionProjection({
    metadata: {
      record: { ...record, revision: expectedRevision ?? record.revision ?? 0 },
      ...(expectedRevision === undefined
        ? {}
        : { expected_revision: expectedRevision }),
    },
    ...(chatLayers === undefined ? {} : { chat_layers: chatLayers }),
  })) as { metadata: RoleplaySessionMetadata };
  return projection.metadata;
}

async function listRoleplayCharacters(
  state: RoleplayRouteContext,
  profileId: string,
): Promise<RoleplayCharacterRecord[]> {
  return state.bridge.listRoleplayCharacters({
    profile_id: profileId,
    page: { limit: 1_000, offset: 0 },
  }) as Promise<RoleplayCharacterRecord[]>;
}

async function getRoleplayCharacter(
  state: RoleplayRouteContext,
  profileId: string,
  characterId: string,
): Promise<RoleplayCharacterRecord | undefined> {
  const record = (await state.bridge.getRoleplayCharacter(characterId)) as
    | RoleplayCharacterRecord
    | undefined;
  return record?.profileId === profileId ? record : undefined;
}

async function requireRoleplayCharacter(
  state: RoleplayRouteContext,
  profileId: string,
  characterId: string,
): Promise<RoleplayCharacterRecord> {
  const character = await getRoleplayCharacter(state, profileId, characterId);
  if (character === undefined) {
    throw new Error(`roleplay character ${characterId} was not found`);
  }
  return character;
}

async function listRoleplayPlayerPersonas(
  state: RoleplayRouteContext,
  profileId: string,
): Promise<RoleplayPlayerPersonaRecord[]> {
  return state.bridge.listRoleplayPlayerPersonas({
    profile_id: profileId,
    page: { limit: 1_000, offset: 0 },
  }) as Promise<RoleplayPlayerPersonaRecord[]>;
}

async function getRoleplayPlayerPersona(
  state: RoleplayRouteContext,
  profileId: string,
  personaId: string,
): Promise<RoleplayPlayerPersonaRecord | undefined> {
  const record = (await state.bridge.getRoleplayPlayerPersona(personaId)) as
    | RoleplayPlayerPersonaRecord
    | undefined;
  return record?.profileId === profileId ? record : undefined;
}

async function requireRoleplayPlayerPersona(
  state: RoleplayRouteContext,
  profileId: string,
  personaId: string,
): Promise<RoleplayPlayerPersonaRecord> {
  const persona = await getRoleplayPlayerPersona(state, profileId, personaId);
  if (persona === undefined) {
    throw new Error(`roleplay player persona ${personaId} was not found`);
  }
  return persona;
}

async function roleplaySessionMetadata(
  state: RoleplayRouteContext,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
): Promise<RoleplaySessionMetadata> {
  const stored = (await state.bridge.getRoleplaySessionMetadata(
    session.sessionId,
  )) as RoleplaySessionMetadata | undefined;
  return {
    sessionId: session.sessionId,
    profileId: session.profileId,
    activeLayerIds: [],
    archived: session.status === "archived",
    revision: 0,
    createdAt: session.createdAt,
    updatedAt: session.lastActiveAt,
    ...(stored ?? {}),
  };
}

async function upsertRoleplaySessionMetadata(
  state: RoleplayRouteContext,
  sessionId: string,
  patch: Partial<RoleplaySessionMetadata>,
): Promise<RoleplaySessionMetadata> {
  const session = await state.serviceSessionById(sessionId);
  const current = await roleplaySessionMetadata(state, session);
  const next: RoleplaySessionMetadata = {
    ...current,
    ...patch,
    sessionId,
    profileId: patch.profileId ?? current.profileId,
    activeLayerIds: patch.activeLayerIds ?? current.activeLayerIds,
    updatedAt: state.now(),
  };
  return putRoleplaySessionMetadataRecord(
    state,
    next,
    current.revision === 0 ? undefined : current.revision,
  );
}

async function roleplaySessionMetadataPatchFromBody(
  state: RoleplayRouteContext,
  current: RoleplaySessionMetadata,
  sessionId: string,
  profileId: string,
  body: Record<string, unknown>,
): Promise<RoleplaySessionMetadataPatchOutput> {
  const requestedPersonaId =
    Object.hasOwn(body, "playerPersonaId") ||
    Object.hasOwn(body, "player_persona_id")
      ? (optionalString(body.playerPersonaId) ??
        optionalString(body.player_persona_id))
      : undefined;
  const requestedCharacterId =
    Object.hasOwn(body, "characterId") || Object.hasOwn(body, "character_id")
      ? (optionalString(body.characterId) ?? optionalString(body.character_id))
      : undefined;
  const activeLayerIdsChanged =
    Object.hasOwn(body, "activeLayerIds") ||
    Object.hasOwn(body, "active_layer_ids");
  const [playerPersona, character, availableLayerIds] = await Promise.all([
    requestedPersonaId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayPlayerPersona(state, profileId, requestedPersonaId),
    requestedCharacterId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayCharacter(state, profileId, requestedCharacterId),
    activeLayerIdsChanged
      ? state.bridge
          .listLoreLayers(profileId)
          .then((layers) => layers.map((layer) => String(layer.layer_id)))
      : Promise.resolve(undefined),
  ]);
  return (await state.bridge.patchRoleplaySessionMetadata({
    current,
    session_id: sessionId,
    profile_id: profileId,
    now: state.now(),
    body,
    player_persona: playerPersona,
    character,
    available_layer_ids: availableLayerIds,
  })) as RoleplaySessionMetadataPatchOutput;
}

function roleplaySessionLifecycleSession(
  session: Pick<
    SessionState,
    | "sessionId"
    | "agentId"
    | "profileId"
    | "kind"
    | "status"
    | "createdAt"
    | "lastActiveAt"
  >,
): RoleplaySessionLifecycleSession {
  return {
    session_id: session.sessionId,
    agent_id: session.agentId,
    profile_id: session.profileId,
    kind: session.kind,
    status: session.status,
    created_at: session.createdAt,
    updated_at: session.lastActiveAt,
  };
}

async function roleplayLifecycleReferencesFromBody(
  state: RoleplayRouteContext,
  profileId: string,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const requestedPersonaId =
    Object.hasOwn(body, "playerPersonaId") ||
    Object.hasOwn(body, "player_persona_id")
      ? (optionalString(body.playerPersonaId) ??
        optionalString(body.player_persona_id))
      : undefined;
  const requestedCharacterId =
    Object.hasOwn(body, "characterId") || Object.hasOwn(body, "character_id")
      ? (optionalString(body.characterId) ?? optionalString(body.character_id))
      : undefined;
  const activeLayerIdsChanged =
    Object.hasOwn(body, "activeLayerIds") ||
    Object.hasOwn(body, "active_layer_ids");
  const [playerPersona, character, availableLayerIds] = await Promise.all([
    requestedPersonaId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayPlayerPersona(state, profileId, requestedPersonaId),
    requestedCharacterId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayCharacter(state, profileId, requestedCharacterId),
    activeLayerIdsChanged
      ? state.bridge
          .listLoreLayers(profileId)
          .then((layers) => layers.map((layer) => String(layer.layer_id)))
      : Promise.resolve(undefined),
  ]);
  return compactRecord({
    player_persona: playerPersona,
    character,
    available_layer_ids: availableLayerIds,
  });
}

async function roleplayLifecycleReferencesFromMetadata(
  state: RoleplayRouteContext,
  metadata: RoleplaySessionMetadata,
): Promise<Record<string, unknown>> {
  const [playerPersona, character, availableLayerIds] = await Promise.all([
    metadata.playerPersonaId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayPlayerPersona(
          state,
          metadata.profileId,
          metadata.playerPersonaId,
        ),
    metadata.characterId === undefined
      ? Promise.resolve(undefined)
      : getRoleplayCharacter(state, metadata.profileId, metadata.characterId),
    metadata.activeLayerIds.length === 0
      ? Promise.resolve(undefined)
      : state.bridge
          .listLoreLayers(metadata.profileId)
          .then((layers) => layers.map((layer) => String(layer.layer_id))),
  ]);
  return compactRecord({
    player_persona: playerPersona,
    character,
    available_layer_ids: availableLayerIds,
  });
}

function roleplayLifecycleChatLayerBindings(
  layers: readonly Record<string, unknown>[],
): RoleplayChatLayerBinding[] {
  return layers.map((layer) => ({
    layer_id: String(layer.layer_id ?? layer.layerId),
    priority: numberValue(layer.priority),
    enabled: layer.enabled !== false,
  }));
}

function roleplayLifecycleSessionKind(
  kind: string,
): "full" | "worker" | "delegated" {
  if (kind === "worker" || kind === "delegated") return kind;
  return "full";
}

async function roleplaySessionSummary(
  state: RoleplayRouteContext,
  session: SessionState,
): Promise<Record<string, unknown>> {
  const metadata = await roleplaySessionMetadata(state, session);
  const playerPersona =
    metadata.playerPersonaId == null
      ? undefined
      : await getRoleplayPlayerPersona(
          state,
          session.profileId,
          metadata.playerPersonaId,
        );
  const character =
    metadata.characterId == null
      ? undefined
      : await getRoleplayCharacter(
          state,
          session.profileId,
          metadata.characterId,
        );
  const chatLayers = await state.bridge
    .getChatLayers(session.sessionId)
    .catch(() => []);
  const activeLayerIds =
    metadata.activeLayerIds.length > 0
      ? metadata.activeLayerIds
      : chatLayers
          .filter((layer) => layer.enabled !== false)
          .sort(
            (left, right) =>
              numberValue(left.priority) - numberValue(right.priority),
          )
          .map((layer) => String(layer.layer_id));
  const lastEvent = (
    await state.listChatEventsAfterCursor(session, undefined, 1)
  ).at(-1);
  return {
    session_id: session.sessionId,
    profile_id: session.profileId,
    agent_id: session.agentId,
    status: session.status,
    display_name: metadata.displayName,
    ...(metadata.playerPersonaId == null
      ? {}
      : { player_persona_id: metadata.playerPersonaId }),
    player_persona_display_name: playerPersona?.displayName ?? "Player",
    player_persona_avatar_url: playerPersona?.avatarUrl,
    player_persona_avatar_asset_ref: playerPersona?.avatarAssetRef,
    player_persona_source: playerPersona === undefined ? "fallback" : "persona",
    ...(metadata.characterId == null
      ? {}
      : { character_id: metadata.characterId }),
    character_name: character?.name,
    active_layer_ids: activeLayerIds,
    active_layer_count: activeLayerIds.length,
    last_message_preview: lastEventPreview(lastEvent),
    archived: metadata.archived || session.status === "archived",
    created_at: metadata.createdAt,
    updated_at: metadata.updatedAt,
    metadata,
  };
}

export async function roleplayPromptContextForSession(
  state: RoleplayRouteContext,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
): Promise<string | undefined> {
  const output = await roleplayPromptContextOutputForSession(state, session);
  return output?.prompt_context;
}

async function roleplayPromptContextOutputForSession(
  state: RoleplayRouteContext,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
): Promise<RoleplayPromptContextOutput | undefined> {
  const metadata = await roleplaySessionMetadata(state, session).catch(
    () => undefined,
  );
  if (metadata === undefined) return undefined;
  const playerPersona =
    metadata.playerPersonaId == null
      ? undefined
      : await getRoleplayPlayerPersona(
          state,
          session.profileId,
          metadata.playerPersonaId,
        ).catch(() => undefined);
  const character =
    metadata.characterId == null
      ? undefined
      : await getRoleplayCharacter(
          state,
          session.profileId,
          metadata.characterId,
        ).catch(() => undefined);
  const output = (await state.bridge.buildRoleplayPromptContext({
    metadata,
    player_persona: playerPersona,
    character,
  })) as RoleplayPromptContextOutput;
  return output;
}

export async function roleplaySpeakerIdentitySnapshotForMessage(
  state: RoleplayRouteContext,
  session: Pick<
    SessionState,
    "sessionId" | "profileId" | "createdAt" | "lastActiveAt" | "status"
  >,
  actor: ChatActor,
  now: string,
): Promise<RoleplaySpeakerIdentitySnapshot> {
  const role =
    actor.kind === "agent"
      ? "assistant"
      : actor.kind === "system"
        ? "system"
        : "user";
  if (role === "system") {
    return (await state.bridge.roleplaySpeakerIdentity({
      actor,
      now,
    })) as RoleplaySpeakerIdentitySnapshot;
  }
  const metadata = await roleplaySessionMetadata(state, session).catch(
    () => undefined,
  );
  if (role === "user") {
    const playerPersona =
      metadata?.playerPersonaId === undefined
        ? undefined
        : await getRoleplayPlayerPersona(
            state,
            session.profileId,
            metadata.playerPersonaId,
          ).catch(() => undefined);
    return (await state.bridge.roleplaySpeakerIdentity({
      actor,
      now,
      metadata,
      player_persona: playerPersona,
    })) as RoleplaySpeakerIdentitySnapshot;
  }
  const character =
    metadata?.characterId === undefined
      ? undefined
      : await getRoleplayCharacter(
          state,
          session.profileId,
          metadata.characterId,
        ).catch(() => undefined);
  return (await state.bridge.roleplaySpeakerIdentity({
    actor,
    now,
    metadata,
    character,
  })) as RoleplaySpeakerIdentitySnapshot;
}

async function listRoleplaySessions(
  state: RoleplayRouteContext,
  profileId: string | undefined,
): Promise<Record<string, unknown>[]> {
  const sessions = (await state.bridge.listSessions()).filter(
    (session) => profileId === undefined || session.profileId === profileId,
  );
  return Promise.all(
    sessions.map((session) => roleplaySessionSummary(state, session)),
  );
}

async function getRoleplaySessionSummary(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<Record<string, unknown> | undefined> {
  const session = (await state.bridge.listSessions()).find(
    (candidate) => candidate.sessionId === sessionId,
  );
  return session === undefined
    ? undefined
    : roleplaySessionSummary(state, session);
}

async function importRoleplayCharacter(
  state: RoleplayRouteContext,
  input: {
    profileId: string;
    importId: string;
    now: string;
    provenance: Record<string, unknown>;
    body?: Record<string, unknown>;
    rawSource?: Record<string, unknown>;
  },
): Promise<RoleplayCharacterRecord | undefined> {
  if (input.body === undefined) return undefined;
  const character = (await state.bridge.writeRoleplayCharacter({
    profile_id: input.profileId,
    now: input.now,
    fallback_id: stableRoleplayRecordId("character", input.importId),
    body: {
      ...input.body,
      id:
        optionalString(input.body.id) ??
        stableRoleplayRecordId(
          "character",
          `${input.profileId}:${optionalString(input.body.name) ?? input.importId}`,
        ),
    },
  })) as RoleplayCharacterRecord;
  return putRoleplayCharacter(state, character);
}

async function importRoleplayPersona(
  state: RoleplayRouteContext,
  input: {
    profileId: string;
    importId: string;
    now: string;
    provenance: Record<string, unknown>;
    body?: Record<string, unknown>;
    rawSource?: Record<string, unknown>;
  },
): Promise<RoleplayPlayerPersonaRecord | undefined> {
  if (input.body === undefined) return undefined;
  const persona = (await state.bridge.writeRoleplayPlayerPersona({
    profile_id: input.profileId,
    now: input.now,
    fallback_id: stableRoleplayRecordId("persona", input.importId),
    body: {
      ...input.body,
      id:
        optionalString(input.body.id) ??
        stableRoleplayRecordId(
          "persona",
          `${input.profileId}:${optionalString(input.body.displayName) ?? optionalString(input.body.name) ?? input.importId}`,
        ),
      displayName:
        optionalString(input.body.displayName) ??
        optionalString(input.body.display_name) ??
        optionalString(input.body.name),
    },
  })) as RoleplayPlayerPersonaRecord;
  return putRoleplayPlayerPersona(state, persona);
}

async function importRoleplayLore(
  state: RoleplayRouteContext,
  input: {
    profileId: string;
    importId: string;
    now: string;
    provenance: Record<string, unknown>;
    layer?: Record<string, unknown>;
    entries: unknown[];
  },
): Promise<{
  layerId?: string;
  entryCount: number;
  createdEntries: number;
  reusedEntries: number;
}> {
  if (input.layer === undefined && input.entries.length === 0) {
    return { entryCount: 0, createdEntries: 0, reusedEntries: 0 };
  }
  const layerId =
    optionalString(input.layer?.layerId) ??
    optionalString(input.layer?.layer_id) ??
    stableRoleplayRecordId("lore-layer", input.importId);
  const existingLayer = await state.bridge.getLoreLayer(layerId);
  if (existingLayer === undefined) {
    await state.bridge.createLoreLayer({
      layer_id: layerId,
      profile_id: input.profileId,
      name:
        optionalString(input.layer?.name) ??
        optionalString(input.layer?.title) ??
        "Imported lorebook",
      description:
        optionalString(input.layer?.description) ??
        "Imported from a normalized SillyTavern packet.",
      purpose: optionalString(input.layer?.purpose) ?? "mixed",
      write_policy: optionalString(input.layer?.writePolicy) ?? "manual",
      now: input.now,
    });
  }
  const linkedRecordIds = new Set(
    (await state.bridge.listEntriesByLayer(layerId)).map((entry) =>
      String(entry.record_id),
    ),
  );
  let createdEntries = 0;
  let reusedEntries = 0;
  for (const rawEntry of input.entries) {
    const entry = recordBody(rawEntry);
    const recordId =
      optionalString(entry.recordId) ??
      optionalString(entry.record_id) ??
      stableRoleplayRecordId(
        "lore",
        `${input.importId}:${optionalString(entry.title) ?? createdEntries + reusedEntries}`,
      );
    let record = await state.bridge.getLoreEntry(recordId);
    if (record === undefined) {
      record = await state.bridge.addLoreEntry(
        roleplayImportedLoreWrite({
          profileId: input.profileId,
          importId: input.importId,
          now: input.now,
          provenance: input.provenance,
          entry,
          recordId,
        }),
      );
      createdEntries += 1;
    } else {
      reusedEntries += 1;
    }
    if (!linkedRecordIds.has(recordId)) {
      await state.bridge.addEntryToLayer({
        layer_id: layerId,
        record_id: String(record.record_id),
        is_constant:
          booleanValue(entry.isConstant) ??
          booleanValue(entry.is_constant) ??
          booleanValue(entry.constant) ??
          false,
        priority: integerValue(entry.priority ?? entry.insertion_order) ?? 0,
        added_at: input.now,
      });
      linkedRecordIds.add(recordId);
    }
  }
  return {
    layerId,
    entryCount: input.entries.length,
    createdEntries,
    reusedEntries,
  };
}

async function importRoleplayTranscript(
  state: RoleplayRouteContext,
  input: {
    profileId: string;
    importId: string;
    now: string;
    provenance: Record<string, unknown>;
    characterId?: string;
    personaId?: string;
    activeLayerIds: string[];
    session?: Record<string, unknown>;
    rows: unknown[];
  },
): Promise<{
  sessionId: string;
  messageCount: number;
  assistantVariantRows: number;
  assistantMultiSwipeRows: number;
  variantCount: number;
}> {
  const sessionId =
    optionalString(input.session?.sessionId) ??
    optionalString(input.session?.session_id) ??
    stableRoleplayRecordId("session", input.importId);
  const existingSession = (await state.bridge.listSessions()).find(
    (session) => session.sessionId === sessionId,
  );
  if (existingSession === undefined) {
    await createRoleplaySession(
      state,
      compactRecord({
        sessionId,
        profileId: input.profileId,
        displayName:
          optionalString(input.session?.displayName) ??
          optionalString(input.session?.display_name) ??
          "Imported ST transcript",
        characterId: input.characterId,
        playerPersonaId: input.personaId,
        activeLayerIds: input.activeLayerIds,
      }),
    );
  } else {
    await updateRoleplaySessionMetadata(
      state,
      sessionId,
      compactRecord({
        displayName:
          optionalString(input.session?.displayName) ??
          optionalString(input.session?.display_name),
        characterId: input.characterId,
        playerPersonaId: input.personaId,
        activeLayerIds: input.activeLayerIds,
      }),
    );
  }
  let previousActiveMessageId: string | undefined;
  let assistantVariantRows = 0;
  let assistantMultiSwipeRows = 0;
  let variantCount = 0;
  for (const [index, rawRow] of input.rows.entries()) {
    const row = recordBody(rawRow);
    const role = roleplayImportRowRole(row);
    const actor: ChatActor =
      role === "assistant"
        ? { id: input.profileId, kind: "agent" }
        : role === "system"
          ? { id: "system", kind: "system" }
          : { id: input.personaId ?? "user", kind: "human" };
    const variants = roleplayImportRowVariants(row);
    const slotId =
      optionalString(row.slotId) ??
      optionalString(row.slot_id) ??
      stableRoleplayRecordId("slot", `${sessionId}:row:${index}`);
    const primaryVariantId =
      optionalString(row.primaryVariantId) ??
      optionalString(row.primary_variant_id) ??
      stableRoleplayRecordId("variant", `${slotId}:0`);
    const activeVariantIndex = Math.min(
      Math.max(
        integerValue(
          row.activeVariantIndex ??
            row.active_variant_index ??
            row.swipeId ??
            row.swipe_id,
        ) ?? 0,
        0,
      ),
      Math.max(variants.length - 1, 0),
    );
    const activeVariantId =
      activeVariantIndex === 0
        ? null
        : stableRoleplayRecordId("variant", `${slotId}:${activeVariantIndex}`);
    await state.bridge.saveMessageSlot({
      slot_id: slotId,
      session_id: sessionId,
      primary_variant_id: primaryVariantId,
      active_variant_id: null,
      metadata_json: {
        source: "st_packet_import",
        import_id: input.importId,
        source_index: index,
        provenance: input.provenance,
      },
      created_at:
        optionalString(row.createdAt) ??
        optionalString(row.created_at) ??
        optionalString(row.send_date) ??
        input.now,
      updated_at: input.now,
    });
    let activeMessageId = "";
    for (const [variantIndex, variantInput] of variants.entries()) {
      const variantId =
        variantIndex === 0
          ? primaryVariantId
          : stableRoleplayRecordId("variant", `${slotId}:${variantIndex}`);
      const messageId =
        optionalString(variantInput.messageId) ??
        optionalString(variantInput.message_id) ??
        stableRoleplayRecordId("message", `${slotId}:${variantIndex}`);
      if (variantIndex === activeVariantIndex) {
        activeMessageId = messageId;
      }
      await state.bridge.saveMessageVariant(
        roleplayMessageVariantWrite({
          sessionId,
          slotId,
          variantId,
          messageId,
          source: variantIndex === 0 ? "primary" : "alternate",
          ordinal: variantIndex,
          actor,
          body: variantInput.body,
          previousMessageId: previousActiveMessageId,
          metadataJson: {
            source: "st_packet_import",
            import_id: input.importId,
            source_index: index,
            source_variant_index: variantIndex,
            active_source_variant_index: activeVariantIndex,
            provenance: input.provenance,
            ...roleplayImportRowMetadata(row),
            ...(variantInput.metadata === undefined
              ? {}
              : { variant_metadata: variantInput.metadata }),
          },
          now:
            optionalString(variantInput.createdAt) ??
            optionalString(row.createdAt) ??
            optionalString(row.created_at) ??
            optionalString(row.send_date) ??
            input.now,
        }),
      );
      variantCount += 1;
    }
    if (activeVariantId !== null) {
      await state.bridge.selectActiveMessageVariant({
        slot_id: slotId,
        active_variant_id: activeVariantId,
        expected: { type: "any" },
        updated_at: input.now,
      });
    }
    previousActiveMessageId =
      activeMessageId ||
      stableRoleplayRecordId("message", `${slotId}:${activeVariantIndex}`);
    if (role === "assistant") {
      assistantVariantRows += 1;
      if (variants.length > 1) assistantMultiSwipeRows += 1;
    }
  }
  return {
    sessionId,
    messageCount: input.rows.length,
    assistantVariantRows,
    assistantMultiSwipeRows,
    variantCount,
  };
}

async function createRoleplaySession(
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const profileId = requiredString(
    body.profileId ?? body.profile_id,
    "profileId",
  );
  const registry = await state.bridge.getProfileRegistryRecord(profileId);
  const now = state.now();
  const fallbackAgentId =
    optionalString(body.agentId) ??
    optionalString(body.agent_id) ??
    registry?.agentId ??
    profileId;
  const fallbackSessionId = `${fallbackAgentId}-rp-${now
    .replace(/[^0-9A-Za-z]/g, "")
    .slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const references = await roleplayLifecycleReferencesFromBody(
    state,
    profileId,
    body,
  );
  const plan = (await state.bridge.planRoleplaySessionLifecycle(
    compactRecord({
      action: "create",
      now,
      body,
      fallback_session_id: fallbackSessionId,
      registry_agent_id: registry?.agentId,
      ...references,
    }),
  )) as RoleplaySessionLifecyclePlan;
  if (!plan.runtime.create_session) {
    throw new Error("roleplay session lifecycle plan did not create a session");
  }
  const session = await state.bridge.createSession({
    sessionId: plan.session_id,
    agentId: plan.agent_id,
    profileId: plan.profile_id,
    kind: roleplayLifecycleSessionKind(plan.kind),
    resourceLimits: {},
    toolProfile: { tools: [] },
  });
  await putRoleplaySessionMetadataRecord(
    state,
    plan.metadata as RoleplaySessionMetadata,
    undefined,
    plan.chat_layer_update === undefined
      ? undefined
      : { ...plan.chat_layer_update, now: state.now() },
  );
  return (
    (await getRoleplaySessionSummary(state, session.sessionId)) ?? {
      session_id: session.sessionId,
      profile_id: session.profileId,
      agent_id: session.agentId,
      status: session.status,
    }
  );
}

async function updateRoleplaySessionMetadata(
  state: RoleplayRouteContext,
  sessionId: string,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const session = await state.serviceSessionById(sessionId);
  const current = await roleplaySessionMetadata(state, session);
  const patch = await roleplaySessionMetadataPatchFromBody(
    state,
    current,
    sessionId,
    session.profileId,
    body,
  );
  await putRoleplaySessionMetadataRecord(
    state,
    patch.metadata,
    current.revision,
    patch.active_layer_ids_changed
      ? {
          chat_id: sessionId,
          layers: patch.metadata.activeLayerIds.map((layerId, index) => ({
            layer_id: layerId,
            priority: index,
            enabled: true,
          })),
          now: state.now(),
        }
      : undefined,
  );
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function archiveRoleplaySession(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<Record<string, unknown>> {
  const session = await state.serviceSessionById(sessionId);
  const current = await roleplaySessionMetadata(state, session);
  const plan = (await state.bridge.planRoleplaySessionLifecycle({
    action: "archive",
    now: state.now(),
    body: {},
    source_session: roleplaySessionLifecycleSession(session),
    current_metadata: current,
  })) as RoleplaySessionLifecyclePlan;
  if (plan.runtime.archive_session) {
    await state.bridge.archiveSession(sessionId as SessionId);
  }
  await putRoleplaySessionMetadataRecord(
    state,
    plan.metadata as RoleplaySessionMetadata,
    current.revision,
  );
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function restoreRoleplaySession(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<Record<string, unknown>> {
  const existing = await state.serviceSessionById(sessionId);
  const current = await roleplaySessionMetadata(state, existing);
  const plan = (await state.bridge.planRoleplaySessionLifecycle({
    action: "restore",
    now: state.now(),
    body: {},
    source_session: roleplaySessionLifecycleSession(existing),
    current_metadata: current,
  })) as RoleplaySessionLifecyclePlan;
  if (plan.runtime.ensure_configured_session) {
    await state.bridge.ensureConfiguredSession({
      sessionId: plan.session_id,
      agentId: plan.agent_id,
      profileId: plan.profile_id,
      kind: roleplayLifecycleSessionKind(plan.kind),
      resourceLimits: compactRecord(
        existing.resourceLimits as unknown as Record<string, unknown>,
      ),
      toolProfile: existing.toolProfile,
      historyWindow: existing.historyWindow,
    });
  }
  await putRoleplaySessionMetadataRecord(
    state,
    plan.metadata as RoleplaySessionMetadata,
    current.revision,
  );
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function forkRoleplaySessionAtMessage(
  state: RoleplayRouteContext,
  sourceSessionId: string,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const sourceSession = await state.serviceSessionById(sourceSessionId);
  const sourceMetadata = await roleplaySessionMetadata(state, sourceSession);
  const now = state.now();
  const fallbackTargetSessionId = `${sourceSession.agentId}-fork-${now
    .replace(/[^0-9A-Za-z]/g, "")
    .slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const [references, sourceLayers] = await Promise.all([
    roleplayLifecycleReferencesFromMetadata(state, sourceMetadata),
    state.bridge.getChatLayers(sourceSessionId),
  ]);
  const plan = (await state.bridge.planRoleplaySessionLifecycle(
    compactRecord({
      action: "fork",
      now,
      body,
      fallback_session_id: fallbackTargetSessionId,
      source_session: roleplaySessionLifecycleSession(sourceSession),
      current_metadata: sourceMetadata,
      source_chat_layers: roleplayLifecycleChatLayerBindings(sourceLayers),
      ...references,
    }),
  )) as RoleplaySessionLifecyclePlan;
  const fork = plan.fork;
  if (fork === undefined) {
    throw new Error("roleplay session lifecycle fork plan is missing");
  }
  if (!plan.runtime.create_session) {
    throw new Error("roleplay session fork plan did not create a session");
  }
  const sourceSlots = await canonicalRoleplaySlotsThroughMessage(
    state,
    sourceSessionId,
    fork.source_message_id,
  );
  const targetSession = await state.bridge.createSession({
    sessionId: plan.session_id,
    agentId: plan.agent_id,
    profileId: plan.profile_id,
    kind: roleplayLifecycleSessionKind(plan.kind),
    resourceLimits: compactRecord(
      sourceSession.resourceLimits as unknown as Record<string, unknown>,
    ),
    toolProfile: sourceSession.toolProfile,
    historyWindow: sourceSession.historyWindow,
  });
  await putRoleplaySessionMetadataRecord(
    state,
    plan.metadata as RoleplaySessionMetadata,
    undefined,
    plan.chat_layer_update === undefined
      ? undefined
      : { ...plan.chat_layer_update, now: state.now() },
  );
  const branch = (await state.bridge.saveConversationBranch({
    branch_id: fork.branch_id,
    session_id: targetSession.sessionId,
    parent_branch_id: null,
    parent_message_id: null,
    origin_message_id: null,
    head_message_id: null,
    label: fork.branch_label,
    metadata_json: fork.branch_metadata_json,
    created_at: now,
    updated_at: now,
  })) as ConversationBranchRecord;

  const copiedMessages = new Map<string, string>();
  let copiedCount = 0;
  let targetCopiedMessageId: string | undefined;
  for (const slot of sourceSlots) {
    const sourceVariant = slot.canonical;
    const sourceMessage = sourceVariant.message;
    const copiedMessageId = stableRoleplayRecordId(
      "message",
      `${targetSession.sessionId}:${sourceMessage.message_id}`,
    );
    copiedMessages.set(sourceMessage.message_id, copiedMessageId);
    if (sourceMessage.message_id === fork.source_message_id) {
      targetCopiedMessageId = copiedMessageId;
    }
    const slotId = stableRoleplayRecordId(
      "slot",
      `${targetSession.sessionId}:${slot.slot.slot_id}`,
    );
    const variantId = stableRoleplayRecordId("variant", slotId);
    await state.bridge.saveMessageSlot({
      slot_id: slotId,
      session_id: targetSession.sessionId,
      primary_variant_id: variantId,
      active_variant_id: null,
      metadata_json: {
        source: "roleplay_session_fork",
        source_slot_id: slot.slot.slot_id,
        source_variant_id: sourceVariant.variant_id,
      },
      created_at: now,
      updated_at: now,
    });
    await state.bridge.saveMessageVariant(
      roleplayMessageVariantWrite({
        sessionId: targetSession.sessionId,
        slotId,
        variantId,
        messageId: copiedMessageId,
        source: "primary",
        ordinal: 0,
        actor: actorForVariant(sourceVariant),
        body: sourceMessage.body,
        branchId: fork.branch_id,
        parentMessageId:
          sourceMessage.parent_message_id === null ||
          sourceMessage.parent_message_id === undefined
            ? undefined
            : copiedMessages.get(sourceMessage.parent_message_id),
        previousMessageId:
          sourceMessage.previous_message_id === null ||
          sourceMessage.previous_message_id === undefined
            ? undefined
            : copiedMessages.get(sourceMessage.previous_message_id),
        metadataJson: {
          ...(optionalRecord(sourceMessage.metadata_json) ?? {}),
          source: "roleplay_session_fork",
          source_session_id: sourceSessionId,
          source_message_id: sourceMessage.message_id,
        },
        now,
      }),
    );
    copiedCount += 1;
  }
  await state.bridge.updateConversationBranchHead({
    branch_id: fork.branch_id,
    head_message_id:
      targetCopiedMessageId ?? [...copiedMessages.values()].at(-1),
    expected: { type: "any" },
    updated_at: state.now(),
  });
  await state.bridge.selectActiveConversationBranch({
    session_id: targetSession.sessionId,
    active_branch_id: fork.branch_id,
    expected: { type: "any" },
    updated_at: state.now(),
  });
  const summary = await getRoleplaySessionSummary(
    state,
    targetSession.sessionId,
  );
  return {
    status: "forked",
    source_session_id: sourceSessionId,
    source_message_id: fork.source_message_id,
    session: summary,
    branch: {
      ...branch,
      head_message_id:
        targetCopiedMessageId ?? [...copiedMessages.values()].at(-1),
    },
    copied_message_count: copiedCount,
  };
}

async function roleplayTerminalAlternativesResult(
  state: RoleplayRouteContext,
  sessionId: string,
  url: URL,
): Promise<Record<string, unknown>> {
  const plan = await roleplayAssistantAlternativePlan(
    state,
    sessionId,
    optionalString(
      url.searchParams.get("slot_id") ?? url.searchParams.get("slotId"),
    ),
  );
  return {
    session_id: sessionId,
    slot: plan.variant_projection,
  };
}

async function createRoleplayAssistantAlternative(
  state: RoleplayRouteContext,
  sessionId: string,
  body: Record<string, unknown>,
  requestIdValue: string,
): Promise<Record<string, unknown>> {
  const plan = await roleplayAssistantAlternativePlan(
    state,
    sessionId,
    optionalString(body.slotId) ?? optionalString(body.slot_id),
    { requestId: requestIdValue, body },
  );
  const write = requiredRoleplayAssistantAlternativeVariantWrite(plan);
  const now = state.now();
  const session = await state.serviceSessionById(sessionId);
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    state,
    session,
    { id: "roleplay-assistant", kind: "agent" },
    now,
  ).catch(() => undefined);
  const bodyText = requiredRouteString(
    optionalString(body.body) ?? optionalString(body.text),
    "body",
  );
  const variant = (await state.bridge.saveMessageVariant(
    roleplayMessageVariantWrite({
      sessionId,
      slotId: write.slot_id,
      variantId: write.variant_id,
      messageId: write.message_id,
      source: "alternate",
      ordinal: write.ordinal,
      actor: { id: "roleplay-assistant", kind: "agent" },
      body: bodyText,
      branchId: write.branch_id ?? undefined,
      parentMessageId: write.parent_message_id ?? undefined,
      previousMessageId: write.previous_message_id ?? undefined,
      metadataJson: {
        source: "roleplay_assistant_alternative",
        generated: false,
        ...(optionalRecord(body.metadata_json) ?? {}),
        ...(speakerIdentity === undefined
          ? {}
          : { speaker_identity: speakerIdentity }),
      },
      now,
    }),
  )) as MessageVariantRecord;
  return {
    status: "created",
    session_id: sessionId,
    slot: roleplayAlternativeSlot({
      ...plan.terminal_slot,
      alternates: [...plan.terminal_slot.alternates, variant],
    }),
    variant,
  };
}

async function generateRoleplayAssistantAlternative(
  state: RoleplayRouteContext,
  sessionId: string,
  body: Record<string, unknown>,
  requestIdValue: string,
): Promise<Record<string, unknown>> {
  if (state.generateRoleplayAssistantAlternative === undefined) {
    throw new Error(
      "roleplay assistant alternative generation is not configured",
    );
  }
  const plan = await roleplayAssistantAlternativePlan(
    state,
    sessionId,
    optionalString(body.slotId) ?? optionalString(body.slot_id),
    { requestId: requestIdValue, body },
  );
  const write = requiredRoleplayAssistantAlternativeVariantWrite(plan);
  const session = await state.serviceSessionById(sessionId);
  const now = state.now();
  const slots = await roleplayMessageSlots(state, sessionId);
  const prompt = roleplayAssistantAlternativeGenerationPrompt(
    slots,
    plan.terminal_slot,
    optionalString(body.instructions),
  );
  const generated = await state.generateRoleplayAssistantAlternative({
    session,
    slot: plan.terminal_slot,
    prompt,
    requestId: requestIdValue,
  });
  const bodyText = requiredRouteString(generated.body.trim(), "generated body");
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    state,
    session,
    { id: "roleplay-assistant", kind: "agent" },
    now,
  ).catch(() => undefined);
  const metadataJson = {
    source: "roleplay_assistant_alternative",
    generated: true,
    generation_source: "model",
    ...(generated.wakeId === undefined ? {} : { wake_id: generated.wakeId }),
    ...(generated.summary === undefined
      ? {}
      : { generation_summary: generated.summary }),
    ...(generated.metadataJson ?? {}),
    ...(optionalRecord(body.metadata_json) ?? {}),
    ...(speakerIdentity === undefined
      ? {}
      : { speaker_identity: speakerIdentity }),
  };
  const variantWrite = roleplayMessageVariantWrite({
    sessionId,
    slotId: write.slot_id,
    variantId: write.variant_id,
    messageId: write.message_id,
    source: "alternate",
    ordinal: write.ordinal,
    actor: { id: "roleplay-assistant", kind: "agent" },
    body: bodyText,
    branchId: write.branch_id ?? undefined,
    parentMessageId: write.parent_message_id ?? undefined,
    previousMessageId: write.previous_message_id ?? undefined,
    metadataJson,
    now,
  });
  const selected = (await state.bridge.applyRoleplayAlternative({
    session_id: sessionId,
    slot_id: write.slot_id,
    create_variant: variantWrite,
    active_variant_id: write.variant_id,
    expected: { type: "any" },
    updated_at: state.now(),
  })) as {
    created_variant?: MessageVariantRecord;
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const variant = selected.created_variant;
  if (variant === undefined)
    throw new Error(
      "roleplay alternative transaction did not create a variant",
    );
  return {
    status: selected.conflict ? "conflict" : "generated",
    session_id: sessionId,
    slot: roleplayAlternativeSlot(selected.slot),
    variant,
    ...(selected.conflict ? { conflict: selected.conflict } : {}),
  };
}

async function selectRoleplayAssistantAlternative(
  state: RoleplayRouteContext,
  sessionId: string,
  slotId: string,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const plan = await roleplayAssistantAlternativePlan(state, sessionId, slotId);
  const activeVariantId =
    optionalString(body.activeVariantId) ??
    optionalString(body.active_variant_id) ??
    optionalString(body.variantId) ??
    optionalString(body.variant_id);
  const result = (await state.bridge.applyRoleplayAlternative({
    session_id: sessionId,
    slot_id: plan.terminal_slot.slot_id,
    active_variant_id: activeVariantId ?? null,
    expected: { type: "any" },
    updated_at: state.now(),
  })) as {
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  return {
    status,
    session_id: sessionId,
    slot: roleplayAlternativeSlot(result.slot),
    ...(result.conflict ? { conflict: result.conflict } : {}),
  };
}

async function canonicalRoleplaySlotsThroughMessage(
  state: RoleplayRouteContext,
  sessionId: string,
  targetMessageId: string,
): Promise<
  Array<{ slot: MessageSlotRecord; canonical: MessageVariantRecord }>
> {
  const slots = await roleplayMessageSlots(state, sessionId);
  const ordered = [...slots].sort((left, right) =>
    left.created_at === right.created_at
      ? left.slot_id.localeCompare(right.slot_id)
      : left.created_at.localeCompare(right.created_at),
  );
  const copied: Array<{
    slot: MessageSlotRecord;
    canonical: MessageVariantRecord;
  }> = [];
  for (const slot of ordered) {
    const explicitTarget = [slot.primary, ...slot.alternates].find(
      (variant) => variant.message.message_id === targetMessageId,
    );
    const canonical = explicitTarget ?? activeVariantForSlot(slot);
    copied.push({ slot, canonical });
    if (explicitTarget !== undefined) {
      return copied;
    }
  }
  throw new Error(
    `message ${targetMessageId} was not found in roleplay session ${sessionId}`,
  );
}

async function roleplayAssistantAlternativePlan(
  state: RoleplayRouteContext,
  sessionId: string,
  slotId: string | undefined,
  writeContext?: { requestId: string; body: Record<string, unknown> },
): Promise<RoleplayAssistantAlternativePlan> {
  const slots = await roleplayMessageSlots(state, sessionId);
  const branchState = (await state.bridge
    .getConversationBranchState({
      session_id: sessionId,
      default_updated_at: state.now(),
    })
    .catch(() => undefined)) as
    | { active_branch_id?: string | null }
    | undefined;
  const branches =
    branchState?.active_branch_id == null
      ? []
      : ((await state.bridge
          .queryConversationBranches({
            session_id: sessionId,
            page: { limit: 500, offset: 0 },
          })
          .catch(() => [])) as ConversationBranchRecord[]);
  return (await state.bridge.planRoleplayAssistantAlternative({
    session_id: sessionId,
    requested_slot_id: slotId,
    request_id: writeContext?.requestId,
    body: writeContext?.body,
    slots,
    active_branch_id: branchState?.active_branch_id ?? null,
    branches,
  })) as RoleplayAssistantAlternativePlan;
}

function requiredRoleplayAssistantAlternativeVariantWrite(
  plan: RoleplayAssistantAlternativePlan,
): NonNullable<RoleplayAssistantAlternativePlan["variant_write"]> {
  if (plan.variant_write === undefined) {
    throw new Error(
      "roleplay assistant alternative variant write plan missing",
    );
  }
  return plan.variant_write;
}

async function roleplayMessageSlots(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<MessageSlotRecord[]> {
  return (await state.bridge.queryMessageSlots({
    session_id: sessionId,
    include_alternates: true,
    page: { limit: 1_000, offset: 0 },
  })) as MessageSlotRecord[];
}

function orderedRoleplaySlots(slots: MessageSlotRecord[]): MessageSlotRecord[] {
  const byPrevious = new Map<string, MessageSlotRecord[]>();
  const roots: MessageSlotRecord[] = [];
  for (const slot of slots) {
    const previous = activeVariantForSlot(slot).message.previous_message_id;
    if (previous == null) {
      roots.push(slot);
      continue;
    }
    const existing = byPrevious.get(previous) ?? [];
    existing.push(slot);
    byPrevious.set(previous, existing);
  }
  const ordered: MessageSlotRecord[] = [];
  const visited = new Set<string>();
  const appendChain = (slot: MessageSlotRecord): void => {
    if (visited.has(slot.slot_id)) return;
    visited.add(slot.slot_id);
    ordered.push(slot);
    const messageId = activeVariantForSlot(slot).message.message_id;
    const children = sortRoleplaySlots(byPrevious.get(messageId) ?? []);
    for (const child of children) appendChain(child);
  };
  for (const root of sortRoleplaySlots(roots)) appendChain(root);
  for (const slot of sortRoleplaySlots(slots)) appendChain(slot);
  return ordered;
}

function sortRoleplaySlots(slots: MessageSlotRecord[]): MessageSlotRecord[] {
  return [...slots].sort((left, right) =>
    left.created_at === right.created_at
      ? left.slot_id.localeCompare(right.slot_id)
      : left.created_at.localeCompare(right.created_at),
  );
}

function roleplayAlternativeSlot(
  slot: MessageSlotRecord,
): RoleplaySessionAlternativeSlot {
  const variants = [slot.primary, ...slot.alternates].filter(
    (variant) => variant.status !== "deleted",
  );
  return {
    slot_id: slot.slot_id,
    active_variant_id: slot.active_variant_id,
    primary_variant_id: slot.primary_variant_id,
    alternate_count: slot.alternates.filter(
      (variant) => variant.status !== "deleted",
    ).length,
    variant_count: variants.length,
    active_variant: activeVariantForSlot(slot),
    variants,
  };
}

function activeVariantForSlot(slot: MessageSlotRecord): MessageVariantRecord {
  if (slot.active_variant_id === null || slot.active_variant_id === undefined) {
    return slot.primary;
  }
  return (
    [slot.primary, ...slot.alternates].find(
      (variant) => variant.variant_id === slot.active_variant_id,
    ) ?? slot.primary
  );
}

function roleplayAssistantAlternativeGenerationPrompt(
  slots: MessageSlotRecord[],
  terminalSlot: MessageSlotRecord,
  instructions: string | undefined,
): string {
  const ordered = orderedRoleplaySlots(slots);
  const terminalIndex = ordered.findIndex(
    (slot) => slot.slot_id === terminalSlot.slot_id,
  );
  const priorSlots =
    terminalIndex < 0 ? ordered : ordered.slice(0, terminalIndex);
  const current = activeVariantForSlot(terminalSlot);
  const transcript = priorSlots
    .map((slot) => roleplayTranscriptLine(activeVariantForSlot(slot)))
    .filter((line): line is string => line !== undefined);
  return [
    "Generate one new alternate assistant reply for the current terminal assistant message.",
    "Return only the assistant message body. Do not include labels, analysis, JSON, markdown fences, or commentary about being an alternative.",
    instructions === undefined
      ? undefined
      : `Additional instructions: ${instructions}`,
    "",
    "# Conversation Before The Assistant Reply",
    transcript.length === 0 ? "(no earlier turns)" : transcript.join("\n\n"),
    "",
    "# Current Assistant Reply To Vary",
    current.message.body,
  ]
    .filter((line): line is string => line !== undefined)
    .join("\n");
}

function roleplayTranscriptLine(
  variant: MessageVariantRecord,
): string | undefined {
  const body = variant.message.body.trim();
  if (!body) return undefined;
  const role =
    variant.message.author_role === "assistant"
      ? "Assistant"
      : variant.message.author_role === "system"
        ? "System"
        : "User";
  return `${role}: ${body}`;
}

function actorForVariant(variant: MessageVariantRecord): ChatActor {
  const role = variant.message.author_role;
  return {
    id: variant.message.author_id,
    kind:
      role === "assistant" ? "agent" : role === "system" ? "system" : "human",
  };
}

function roleplayMessageVariantWrite(input: {
  sessionId: string;
  slotId: string;
  variantId: string;
  messageId: string;
  source: "primary" | "alternate";
  ordinal: number;
  actor: ChatActor;
  body: string;
  branchId?: string | null;
  parentMessageId?: string | null;
  previousMessageId?: string | null;
  metadataJson: unknown;
  now: string;
}): Record<string, unknown> {
  return {
    variant_id: input.variantId,
    slot_id: input.slotId,
    source: input.source,
    ordinal: input.ordinal,
    status: "active",
    message: {
      message_id: input.messageId,
      session_id: input.sessionId,
      branch_id: input.branchId ?? null,
      parent_message_id: input.parentMessageId ?? null,
      previous_message_id: input.previousMessageId ?? null,
      author_id: input.actor.id,
      author_role:
        input.actor.kind === "agent"
          ? "assistant"
          : input.actor.kind === "system"
            ? "system"
            : "user",
      status: "completed",
      body: input.body,
      metadata_json: input.metadataJson ?? {},
      created_at: input.now,
      blocks: [
        {
          block_id: `${input.messageId}:block:1`,
          ordinal: 0,
          kind: "text",
          content_json: { text: input.body },
          render_policy_json: undefined,
          metadata_json: {},
        },
      ],
    },
    metadata_json: input.metadataJson ?? {},
    created_at: input.now,
    updated_at: input.now,
  };
}

function roleplayImportedLoreWrite(input: {
  profileId: string;
  importId: string;
  now: string;
  provenance: Record<string, unknown>;
  entry: Record<string, unknown>;
  recordId: string;
}): Record<string, unknown> {
  const controls = optionalRecord(input.entry.controls) ?? {};
  const rawMetadata =
    optionalRecord(input.entry.rawMetadata) ??
    optionalRecord(input.entry.raw_metadata) ??
    optionalRecord(input.entry.stMetadata) ??
    optionalRecord(input.entry.st_metadata) ??
    {};
  return {
    record_id: input.recordId,
    world_id: optionalString(input.entry.worldId) ?? input.profileId,
    entity_id: optionalString(input.entry.entityId),
    session_id: optionalString(input.entry.sessionId),
    branch_id: optionalString(input.entry.branchId),
    shape: {
      shape_id: optionalString(input.entry.shapeId) ?? "lore_entry",
      version: integerValue(input.entry.shapeVersion) ?? 1,
    },
    canon_status: optionalString(input.entry.canonStatus) ?? "draft",
    visibility: optionalString(input.entry.visibility) ?? "public",
    title:
      optionalString(input.entry.title) ??
      optionalString(input.entry.name) ??
      input.recordId,
    body:
      optionalString(input.entry.body) ??
      optionalString(input.entry.contentText) ??
      "",
    content: {
      ...(optionalRecord(input.entry.content) ?? {}),
      world_id: optionalString(input.entry.worldId) ?? input.profileId,
      entity_id: optionalString(input.entry.entityId),
      title:
        optionalString(input.entry.title) ??
        optionalString(input.entry.name) ??
        input.recordId,
      body:
        optionalString(input.entry.body) ??
        optionalString(input.entry.contentText) ??
        "",
      canon_status: optionalString(input.entry.canonStatus) ?? "draft",
      visibility: optionalString(input.entry.visibility) ?? "public",
      lore_controls: {
        primary_keys: stringArray(
          input.entry.primaryKeys ??
            input.entry.primary_keys ??
            input.entry.keys ??
            controls.primaryKeys ??
            controls.primary_keys,
        ),
        secondary_keys: stringArray(
          input.entry.secondaryKeys ??
            input.entry.secondary_keys ??
            controls.secondaryKeys ??
            controls.secondary_keys,
        ),
        enabled: booleanValue(input.entry.enabled ?? controls.enabled) ?? true,
        constant:
          booleanValue(
            input.entry.constant ??
              input.entry.isConstant ??
              input.entry.is_constant ??
              controls.constant,
          ) ?? false,
        scan_depth:
          integerValue(input.entry.scanDepth ?? input.entry.scan_depth) ?? 4,
        insertion_position:
          optionalString(
            input.entry.insertionPosition ?? input.entry.insertion_position,
          ) ?? "lore_block",
        insertion_order:
          integerValue(
            input.entry.insertionOrder ?? input.entry.insertion_order,
          ) ?? 0,
        probability:
          optionalNumberValue(
            input.entry.probability ?? controls.probability,
          ) ?? 1,
        retrieval_role:
          optionalString(
            input.entry.retrievalRole ?? input.entry.retrieval_role,
          ) ?? "system",
      },
      metadata_json: {
        ...(optionalRecord(
          optionalRecord(input.entry.content)?.metadata_json,
        ) ?? {}),
        source: "st_packet_import",
        import_id: input.importId,
        provenance: input.provenance,
        st_metadata: rawMetadata,
      },
    },
    evidence_refs: [
      {
        evidence_type: "import",
        ref_id: input.importId,
        label: "SillyTavern packet import",
      },
    ],
    source: "import",
    confidence: optionalNumberValue(input.entry.confidence) ?? 0.85,
    durability_rationale:
      optionalString(input.entry.durabilityRationale) ??
      optionalString(input.entry.durability_rationale) ??
      "Imported from a normalized SillyTavern packet.",
    supersedes_record_id: optionalString(input.entry.supersedesRecordId),
    now: input.now,
  };
}

function roleplayImportRowRole(row: Record<string, unknown>): string {
  const role = optionalString(row.role);
  if (role === "assistant" || role === "system" || role === "user") {
    return role;
  }
  if (booleanValue(row.isUser ?? row.is_user) === true) return "user";
  if (booleanValue(row.isSystem ?? row.is_system) === true) return "system";
  return "assistant";
}

function roleplayImportRowVariants(row: Record<string, unknown>): Array<{
  body: string;
  messageId?: string;
  message_id?: string;
  createdAt?: string;
  metadata?: unknown;
}> {
  const explicit = arrayValue(row.variants)
    .map((variant) =>
      isRecord(variant)
        ? {
            body:
              optionalString(variant.body) ??
              optionalString(variant.text) ??
              "",
            messageId: optionalString(variant.messageId),
            message_id: optionalString(variant.message_id),
            createdAt: optionalString(variant.createdAt),
            metadata:
              variant.metadata ?? variant.metadata_json ?? variant.extra,
          }
        : { body: typeof variant === "string" ? variant : "" },
    )
    .filter((variant) => variant.body.length > 0);
  if (explicit.length > 0) return explicit;
  const swipeInfo = arrayValue(row.swipe_info);
  const swipes = arrayValue(row.swipes)
    .map((swipe, index) => ({
      body:
        typeof swipe === "string"
          ? swipe
          : (optionalString(optionalRecord(swipe)?.body) ??
            optionalString(optionalRecord(swipe)?.text) ??
            ""),
      metadata: swipeInfo[index],
    }))
    .filter((variant) => variant.body.length > 0);
  if (swipes.length > 0) return swipes;
  return [
    {
      body:
        optionalString(row.body) ??
        optionalString(row.text) ??
        optionalString(row.mes) ??
        "",
    },
  ].filter((variant) => variant.body.length > 0);
}

function roleplayImportRowMetadata(
  row: Record<string, unknown>,
): Record<string, unknown> {
  return compactRecord({
    st_name: row.name,
    st_send_date: row.send_date,
    st_extra: row.extra,
    st_model: row.model ?? optionalRecord(row.extra)?.model,
    st_api: row.api ?? optionalRecord(row.extra)?.api,
    st_reasoning:
      row.reasoning ??
      row.reasoning_trace ??
      optionalRecord(row.extra)?.reasoning,
    st_reasoning_type:
      row.reasoning_type ?? optionalRecord(row.extra)?.reasoning_type,
    st_reasoning_duration:
      row.reasoning_duration ?? optionalRecord(row.extra)?.reasoning_duration,
    st_time_to_first_token:
      row.time_to_first_token ?? optionalRecord(row.extra)?.time_to_first_token,
  });
}

function stableRoleplayRecordId(prefix: string, raw: string): string {
  return `${prefix}:${raw.replace(/[^A-Za-z0-9._:-]+/g, "_").slice(0, 160)}`;
}

function lastEventPreview(event: ChatEvent | undefined): string | undefined {
  if (event === undefined) return undefined;
  const body =
    optionalString(event.payload.body) ??
    optionalString(event.payload.text) ??
    optionalString(event.payload.summary);
  return body === undefined ? undefined : body.slice(0, 180);
}

async function readRoleplayNarratorConfig(
  state: RoleplayRouteContext,
  profileId: string,
): Promise<BrowserRoleplayNarratorConfig> {
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  return normalizeRoleplayNarratorConfig(state, profile.roleplayNarrator ?? {});
}

async function writeRoleplayNarratorConfig(
  state: RoleplayRouteContext,
  profileId: string,
  body: Record<string, unknown>,
): Promise<BrowserRoleplayNarratorConfig> {
  const config = await normalizeRoleplayNarratorConfig(
    state,
    body.config ?? body,
  );
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const raw = JSON.parse(await readFile(profilePath, "utf8")) as unknown;
  if (!isRecord(raw)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  if (isRecord(raw.roleplayMechanic)) {
    throw new Error(
      `profile ${profileId} is configured as a mechanic; create a separate narrator profile`,
    );
  }
  await writeJsonFileAtomic(profilePath, {
    ...raw,
    profileId,
    roleplayNarrator: config,
  });
  await state.applyServiceRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "roleplay_narrator_config_updated",
    summaryPrefix: `Roleplay narrator config for ${profileId} updated`,
  });
  await state.rebuildBrainRuntime(profileId as ProfileId);
  return config;
}

async function normalizeRoleplayNarratorConfig(
  state: RoleplayRouteContext,
  input: unknown,
): Promise<BrowserRoleplayNarratorConfig> {
  return (await state.bridge.normalizeRoleplayNarratorConfig(
    recordBody(input),
  )) as BrowserRoleplayNarratorConfig;
}

async function readRoleplayMechanicConfig(
  state: RoleplayRouteContext,
  profileId: string,
): Promise<{
  configured: boolean;
  config: BrowserRoleplayMechanicProfilePlan["config"];
  localToolProfileId: string;
  toolPolicyIsolated: boolean;
}> {
  const profile = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  const plan = await planRoleplayMechanicProfile(state, {
    name: profile.displayName ?? profile.profileId,
    providerAlias: profile.providerAlias,
    autoMonitor: profile.roleplayMechanic?.autoMonitor ?? false,
  });
  return {
    configured: profile.roleplayMechanic !== undefined,
    config: plan.config,
    localToolProfileId: plan.localToolProfileId,
    toolPolicyIsolated:
      profile.localToolProfileId === plan.localToolProfileId &&
      profile.toolPolicy?.requestedToolsets?.includes("roleplay_mechanic") ===
        true,
  };
}

async function writeRoleplayMechanicConfig(
  state: RoleplayRouteContext,
  profileId: string,
  body: Record<string, unknown>,
): Promise<{
  configured: true;
  config: BrowserRoleplayMechanicProfilePlan["config"];
  localToolProfileId: string;
  toolPolicyIsolated: true;
}> {
  const profilePath = safeProfileConfigPath(
    state.runtimeConfig.profilesDir,
    profileId,
  );
  if (profilePath === undefined) {
    throw new Error(`profile id ${profileId} is not a valid file profile id`);
  }
  const raw = JSON.parse(await readFile(profilePath, "utf8")) as unknown;
  if (!isRecord(raw)) {
    throw new Error(`profile ${profileId} config root must be an object`);
  }
  const current = await loadProfileConfig(
    state.runtimeConfig.profilesDir,
    profileId as ProfileId,
  );
  if (current.roleplayNarrator !== undefined) {
    throw new Error(
      `profile ${profileId} is configured as a narrator; create a separate mechanic profile`,
    );
  }
  const configBody = isRecord(body.config) ? body.config : body;
  const requestedProviderAlias =
    optionalString(configBody.providerAlias ?? configBody.provider_alias) ??
    current.providerAlias;
  if (requestedProviderAlias !== undefined) {
    const provider = await state.bridge.getModelProvider(
      requestedProviderAlias,
    );
    if (provider === undefined) {
      throw new Error(
        `model provider alias ${requestedProviderAlias} was not found`,
      );
    }
    if (provider.status !== "active") {
      throw new Error(
        `model provider alias ${requestedProviderAlias} is ${provider.status}; active provider required`,
      );
    }
  }
  const plan = await planRoleplayMechanicProfile(state, {
    name:
      optionalString(configBody.name ?? configBody.displayName) ??
      current.displayName ??
      profileId,
    providerAlias: requestedProviderAlias,
    autoMonitor:
      configBody.autoMonitor ??
      configBody.auto_monitor ??
      current.roleplayMechanic?.autoMonitor ??
      false,
  });
  const localToolProfile = await createLocalToolProfileStore({
    bridge: state.bridge,
    now: state.now,
  }).resolve(plan.localToolProfileId);
  await writeJsonFileAtomic(profilePath, {
    ...raw,
    profileId,
    displayName: plan.config.name,
    ...(plan.config.providerAlias === undefined
      ? {}
      : { providerAlias: plan.config.providerAlias }),
    localToolProfileId: localToolProfile.id,
    toolPolicy: localToolProfile.toolPolicy,
    roleplayMechanic: { autoMonitor: false },
  });
  await state.applyServiceRuntimeConfigFromDisk({
    createMissingSessions: false,
    eventType: "roleplay_mechanic_config_updated",
    summaryPrefix: `Roleplay mechanic config for ${profileId} updated`,
  });
  await state.rebuildBrainRuntime(profileId as ProfileId);
  return {
    configured: true,
    config: plan.config,
    localToolProfileId: plan.localToolProfileId,
    toolPolicyIsolated: true,
  };
}

async function planRoleplayMechanicProfile(
  state: RoleplayRouteContext,
  input: unknown,
): Promise<BrowserRoleplayMechanicProfilePlan> {
  return (await state.bridge.planRoleplayMechanicProfile(
    input,
  )) as BrowserRoleplayMechanicProfilePlan;
}

function numberValue(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function roleplayNotFound(
  requestIdValue: string,
  reasonCode: string,
  message: string,
): AdminRouteResult {
  return failure(404, requestIdValue, {
    code: "not_found",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}

function roleplaySuccess<T>(
  requestIdValue: string,
  data: T,
  status: number,
): AdminRouteResult<T> {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestIdValue, schema_version: 1 },
    },
  };
}

function roleplayInputError(
  requestIdValue: string,
  reasonCode: string,
  error: unknown,
): AdminRouteResult {
  return failure(400, requestIdValue, {
    code: "invalid_input",
    reason_code: reasonCode,
    message: errorMessage(error, "roleplay request failed"),
    retryable: false,
  });
}

function roleplayLoreMethodNotAllowed(
  requestIdValue: string,
  message: string,
): AdminRouteResult {
  return failure(405, requestIdValue, {
    code: "method_not_allowed",
    reason_code: "roleplay_method_not_allowed",
    message,
    retryable: false,
  });
}

function requiredRouteString(
  value: string | undefined,
  fieldName: string,
): string {
  if (!value) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredRouteBoolean(
  value: boolean | undefined,
  fieldName: string,
): boolean {
  if (value === undefined) throw new Error(`${fieldName} is required`);
  return value;
}

function requiredPositiveInteger(value: unknown, fieldName: string): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error(`${fieldName} must be a positive integer`);
  }
  return parsed;
}

function safeProfileConfigPath(
  profilesDir: string,
  profileId: string,
): string | undefined {
  if (!/^[A-Za-z0-9][A-Za-z0-9._:-]*$/.test(profileId)) {
    return undefined;
  }
  return join(profilesDir, `${profileId}.json`);
}

async function writeJsonFileAtomic(
  path: string,
  value: unknown,
): Promise<void> {
  await mkdir(dirname(path), { recursive: true });
  const tmpPath = `${path}.${process.pid}.${Date.now()}.tmp`;
  await writeFile(tmpPath, `${JSON.stringify(value, null, 2)}\n`);
  await rename(tmpPath, path);
}

function arrayValue(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function booleanValue(value: unknown): boolean | undefined {
  if (typeof value === "boolean") return value;
  if (value === "true") return true;
  if (value === "false") return false;
  return undefined;
}

function integerValue(value: unknown): number | undefined {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isSafeInteger(parsed) ? parsed : undefined;
}

function optionalNumberValue(value: unknown): number | undefined {
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function stringArray(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value.filter((item): item is string => typeof item === "string");
  }
  if (typeof value === "string" && value.length > 0) {
    return value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);
  }
  return [];
}

function requiredString(value: unknown, fieldName: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${fieldName} is required`);
  }
  return value;
}

function recordBody(value: unknown): Record<string, unknown> {
  if (value === undefined || value === null) return {};
  if (!isRecord(value)) throw new Error("request body must be a JSON object");
  return value;
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return isRecord(value) ? value : undefined;
}

function compactRecord(
  record: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(record).filter(
      (entry): entry is [string, unknown] =>
        entry[1] !== null && entry[1] !== undefined,
    ),
  );
}

function requestId(request: IncomingMessage): string {
  const header = request.headers["x-request-id"];
  if (typeof header === "string" && header.length > 0) return header;
  if (Array.isArray(header) && header[0]) return header[0];
  return randomBytes(8).toString("hex");
}

async function readJsonBody(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];
  for await (const chunk of request) {
    chunks.push(typeof chunk === "string" ? Buffer.from(chunk) : chunk);
  }
  if (chunks.length === 0) return {};
  const text = Buffer.concat(chunks).toString("utf8");
  if (text.trim().length === 0) return {};
  return JSON.parse(text) as unknown;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error && error.message.length > 0
    ? error.message
    : fallback;
}
