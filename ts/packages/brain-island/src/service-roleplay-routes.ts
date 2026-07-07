import type { IncomingMessage } from "node:http";
import { randomBytes } from "node:crypto";
import { mkdir, readFile, rename, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import type { ProfileId, SessionId, SessionState } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeSimpleKvRecord,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { failure, successRoute } from "./service-route-results.js";
import { loadProfileConfig } from "./profile-loading.js";
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
  serviceSessionById(sessionId: string): Promise<SessionState>;
  listChatEventsAfterCursor(
    session: SessionState,
    afterCursor: string | undefined,
    limit: number,
  ): readonly ChatEvent[];
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
  createdAt: string;
  updatedAt?: string;
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
  createdAt: string;
  updatedAt?: string;
}

interface RoleplaySessionMetadata {
  sessionId: string;
  profileId: string;
  displayName?: string;
  playerPersonaId?: string;
  characterId?: string;
  activeLayerIds: string[];
  archived: boolean;
  createdAt: string;
  updatedAt: string;
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
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/sessions")) {
    return handleRoleplaySessionRequest(request, state, url);
  }
  if (url.pathname.startsWith("/v1/admin/roleplay/lore/")) {
    return handleAdminRoleplayLoreRequest(request, state, url, {
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
        const character = roleplayCharacterFromBody(
          recordBody(await readJsonBody(request)),
          profileId,
          state.now(),
        );
        await putRoleplayJson(state, roleplayCharacterScope(profileId), {
          key: roleplayCharacterKey(character.id),
          value: character,
        });
        return successRoute(requestIdValue, { character });
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
      const character = mergeRoleplayCharacter(
        current,
        recordBody(await readJsonBody(request)),
        state.now(),
      );
      await putRoleplayJson(state, roleplayCharacterScope(profileId), {
        key: roleplayCharacterKey(character.id),
        value: character,
      });
      return successRoute(requestIdValue, { character });
    }
    if (method === "DELETE") {
      const current = await requireRoleplayCharacter(
        state,
        profileId,
        characterId,
      );
      const character = {
        ...current,
        status: "archived" as const,
        updatedAt: state.now(),
      };
      await putRoleplayJson(state, roleplayCharacterScope(profileId), {
        key: roleplayCharacterKey(character.id),
        value: character,
      });
      return successRoute(requestIdValue, { character });
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
        const persona = roleplayPlayerPersonaFromBody(
          recordBody(await readJsonBody(request)),
          profileId,
          state.now(),
        );
        await putRoleplayJson(state, roleplayPlayerPersonaScope(profileId), {
          key: roleplayPlayerPersonaKey(persona.id),
          value: persona,
        });
        return successRoute(requestIdValue, { persona });
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
      const persona = mergeRoleplayPlayerPersona(
        current,
        recordBody(await readJsonBody(request)),
        state.now(),
      );
      await putRoleplayJson(state, roleplayPlayerPersonaScope(profileId), {
        key: roleplayPlayerPersonaKey(persona.id),
        value: persona,
      });
      return successRoute(requestIdValue, { persona });
    }
    if (method === "DELETE") {
      const current = await requireRoleplayPlayerPersona(
        state,
        profileId,
        personaId,
      );
      const persona = {
        ...current,
        status: "archived" as const,
        updatedAt: state.now(),
      };
      await putRoleplayJson(state, roleplayPlayerPersonaScope(profileId), {
        key: roleplayPlayerPersonaKey(persona.id),
        value: persona,
      });
      return successRoute(requestIdValue, { persona });
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

function roleplayCharacterScope(profileId: string): {
  scopeType: string;
  scopeId: string;
} {
  return { scopeType: "roleplay_profile", scopeId: profileId };
}

function roleplayPlayerPersonaScope(profileId: string): {
  scopeType: string;
  scopeId: string;
} {
  return { scopeType: "roleplay_profile", scopeId: profileId };
}

function roleplaySessionScope(sessionId: string): {
  scopeType: string;
  scopeId: string;
} {
  return { scopeType: "roleplay_session", scopeId: sessionId };
}

function roleplayCharacterKey(characterId: string): string {
  return `character:${characterId}`;
}

function roleplayPlayerPersonaKey(personaId: string): string {
  return `player_persona:${personaId}`;
}

function roleplaySessionMetadataKey(): string {
  return "metadata";
}

async function putRoleplayJson(
  state: RoleplayRouteContext,
  scope: { scopeType: string; scopeId: string },
  input: { key: string; value: unknown },
): Promise<NativeSimpleKvRecord> {
  return state.bridge.putSimpleKv({
    ...scope,
    key: input.key,
    valueJson: JSON.stringify(input.value),
    now: state.now(),
  });
}

async function listRoleplayJson<T>(
  state: RoleplayRouteContext,
  scope: { scopeType: string; scopeId: string },
  keyPrefix: string,
): Promise<T[]> {
  const records = await state.bridge.listSimpleKv({
    ...scope,
    keyPrefix,
    limit: 1_000,
    offset: 0,
  });
  return records.map((record) => JSON.parse(record.valueJson) as T);
}

async function getRoleplayJson<T>(
  state: RoleplayRouteContext,
  scope: { scopeType: string; scopeId: string },
  key: string,
): Promise<T | undefined> {
  const records = await state.bridge.listSimpleKv({
    ...scope,
    keyPrefix: key,
    limit: 1,
    offset: 0,
  });
  const exact = records.find((record) => record.key === key);
  return exact === undefined ? undefined : (JSON.parse(exact.valueJson) as T);
}

function roleplayCharacterFromBody(
  body: Record<string, unknown>,
  profileId: string,
  now: string,
): RoleplayCharacterRecord {
  const id =
    optionalString(body.id) ??
    optionalString(body.character_id) ??
    optionalString(body.characterId) ??
    `character-${randomBytes(6).toString("hex")}`;
  return {
    id,
    profileId,
    name: requiredString(body.name, "name"),
    description: optionalString(body.description) ?? "",
    personality: optionalString(body.personality) ?? "",
    scenario: optionalString(body.scenario) ?? "",
    firstMessage:
      optionalString(body.firstMessage) ??
      optionalString(body.first_message) ??
      "",
    alternateGreetings: optionalStringArray(
      body.alternateGreetings ?? body.alternate_greetings,
      [],
      "alternateGreetings",
    ),
    exampleMessages: optionalStringArray(
      body.exampleMessages ?? body.example_messages,
      [],
      "exampleMessages",
    ),
    tags: optionalStringArray(body.tags, [], "tags"),
    ...((optionalString(body.avatarUrl) ?? optionalString(body.avatar_url))
      ? {
          avatarUrl:
            optionalString(body.avatarUrl) ?? optionalString(body.avatar_url),
        }
      : {}),
    status: "active",
    createdAt: now,
    updatedAt: now,
  };
}

function mergeRoleplayCharacter(
  current: RoleplayCharacterRecord,
  body: Record<string, unknown>,
  now: string,
): RoleplayCharacterRecord {
  return {
    ...current,
    ...(optionalString(body.name) === undefined
      ? {}
      : { name: optionalString(body.name) }),
    ...(Object.hasOwn(body, "description")
      ? { description: optionalString(body.description) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "personality")
      ? { personality: optionalString(body.personality) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "scenario")
      ? { scenario: optionalString(body.scenario) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "firstMessage") ||
    Object.hasOwn(body, "first_message")
      ? {
          firstMessage:
            optionalString(body.firstMessage) ??
            optionalString(body.first_message) ??
            "",
        }
      : {}),
    ...(body.alternateGreetings !== undefined ||
    body.alternate_greetings !== undefined
      ? {
          alternateGreetings: optionalStringArray(
            body.alternateGreetings ?? body.alternate_greetings,
            [],
            "alternateGreetings",
          ),
        }
      : {}),
    ...(body.exampleMessages !== undefined ||
    body.example_messages !== undefined
      ? {
          exampleMessages: optionalStringArray(
            body.exampleMessages ?? body.example_messages,
            [],
            "exampleMessages",
          ),
        }
      : {}),
    ...(body.tags === undefined
      ? {}
      : { tags: optionalStringArray(body.tags, [], "tags") }),
    ...(body.avatarUrl !== undefined || body.avatar_url !== undefined
      ? {
          avatarUrl:
            optionalString(body.avatarUrl) ?? optionalString(body.avatar_url),
        }
      : {}),
    status:
      optionalString(body.status) === "archived" ? "archived" : current.status,
    updatedAt: now,
  };
}

function roleplayPlayerPersonaFromBody(
  body: Record<string, unknown>,
  profileId: string,
  now: string,
): RoleplayPlayerPersonaRecord {
  const id =
    optionalString(body.id) ??
    optionalString(body.persona_id) ??
    optionalString(body.personaId) ??
    `persona-${randomBytes(6).toString("hex")}`;
  const avatarUrl =
    optionalString(body.avatarUrl) ?? optionalString(body.avatar_url);
  const avatarAssetRef =
    optionalString(body.avatarAssetRef) ??
    optionalString(body.avatar_asset_ref) ??
    optionalString(body.assetRef) ??
    optionalString(body.asset_ref);
  return {
    id,
    profileId,
    displayName: requiredString(
      body.displayName ?? body.display_name ?? body.name,
      "displayName",
    ),
    ...(avatarUrl ? { avatarUrl } : {}),
    ...(avatarAssetRef ? { avatarAssetRef } : {}),
    description: optionalString(body.description) ?? "",
    notes: optionalString(body.notes) ?? "",
    status: "active",
    createdAt: now,
    updatedAt: now,
  };
}

function mergeRoleplayPlayerPersona(
  current: RoleplayPlayerPersonaRecord,
  body: Record<string, unknown>,
  now: string,
): RoleplayPlayerPersonaRecord {
  return {
    ...current,
    ...(Object.hasOwn(body, "displayName") ||
    Object.hasOwn(body, "display_name") ||
    Object.hasOwn(body, "name")
      ? {
          displayName: requiredString(
            body.displayName ?? body.display_name ?? body.name,
            "displayName",
          ),
        }
      : {}),
    ...(Object.hasOwn(body, "avatarUrl") || Object.hasOwn(body, "avatar_url")
      ? {
          avatarUrl:
            optionalString(body.avatarUrl) ?? optionalString(body.avatar_url),
        }
      : {}),
    ...(Object.hasOwn(body, "avatarAssetRef") ||
    Object.hasOwn(body, "avatar_asset_ref") ||
    Object.hasOwn(body, "assetRef") ||
    Object.hasOwn(body, "asset_ref")
      ? {
          avatarAssetRef:
            optionalString(body.avatarAssetRef) ??
            optionalString(body.avatar_asset_ref) ??
            optionalString(body.assetRef) ??
            optionalString(body.asset_ref),
        }
      : {}),
    ...(Object.hasOwn(body, "description")
      ? { description: optionalString(body.description) ?? "" }
      : {}),
    ...(Object.hasOwn(body, "notes")
      ? { notes: optionalString(body.notes) ?? "" }
      : {}),
    status:
      optionalString(body.status) === "archived" ? "archived" : current.status,
    updatedAt: now,
  };
}

async function listRoleplayCharacters(
  state: RoleplayRouteContext,
  profileId: string,
): Promise<RoleplayCharacterRecord[]> {
  return listRoleplayJson<RoleplayCharacterRecord>(
    state,
    roleplayCharacterScope(profileId),
    "character:",
  );
}

async function getRoleplayCharacter(
  state: RoleplayRouteContext,
  profileId: string,
  characterId: string,
): Promise<RoleplayCharacterRecord | undefined> {
  return getRoleplayJson<RoleplayCharacterRecord>(
    state,
    roleplayCharacterScope(profileId),
    roleplayCharacterKey(characterId),
  );
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
  return listRoleplayJson<RoleplayPlayerPersonaRecord>(
    state,
    roleplayPlayerPersonaScope(profileId),
    "player_persona:",
  );
}

async function getRoleplayPlayerPersona(
  state: RoleplayRouteContext,
  profileId: string,
  personaId: string,
): Promise<RoleplayPlayerPersonaRecord | undefined> {
  return getRoleplayJson<RoleplayPlayerPersonaRecord>(
    state,
    roleplayPlayerPersonaScope(profileId),
    roleplayPlayerPersonaKey(personaId),
  );
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
  const stored = await getRoleplayJson<RoleplaySessionMetadata>(
    state,
    roleplaySessionScope(session.sessionId),
    roleplaySessionMetadataKey(),
  );
  return {
    sessionId: session.sessionId,
    profileId: session.profileId,
    activeLayerIds: [],
    archived: session.status === "archived",
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
  await putRoleplayJson(state, roleplaySessionScope(sessionId), {
    key: roleplaySessionMetadataKey(),
    value: next,
  });
  return next;
}

async function roleplaySessionSummary(
  state: RoleplayRouteContext,
  session: SessionState,
): Promise<Record<string, unknown>> {
  const metadata = await roleplaySessionMetadata(state, session);
  const playerPersona =
    metadata.playerPersonaId === undefined
      ? undefined
      : await getRoleplayPlayerPersona(
          state,
          session.profileId,
          metadata.playerPersonaId,
        );
  const character =
    metadata.characterId === undefined
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
  const lastEvent = state
    .listChatEventsAfterCursor(session, undefined, 1)
    .at(-1);
  return {
    session_id: session.sessionId,
    profile_id: session.profileId,
    agent_id: session.agentId,
    status: session.status,
    display_name: metadata.displayName,
    player_persona_id: metadata.playerPersonaId,
    player_persona_display_name: playerPersona?.displayName ?? "Player",
    player_persona_avatar_url: playerPersona?.avatarUrl,
    player_persona_avatar_asset_ref: playerPersona?.avatarAssetRef,
    player_persona_source: playerPersona === undefined ? "fallback" : "persona",
    character_id: metadata.characterId,
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
  const metadata = await roleplaySessionMetadata(state, session).catch(
    () => undefined,
  );
  if (metadata === undefined) return undefined;
  const playerPersona =
    metadata.playerPersonaId === undefined
      ? undefined
      : await getRoleplayPlayerPersona(
          state,
          session.profileId,
          metadata.playerPersonaId,
        ).catch(() => undefined);
  const character =
    metadata.characterId === undefined
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
  })) as { prompt_context?: string };
  return output.prompt_context;
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

async function createRoleplaySession(
  state: RoleplayRouteContext,
  body: Record<string, unknown>,
): Promise<Record<string, unknown>> {
  const profileId = requiredString(
    body.profileId ?? body.profile_id,
    "profileId",
  );
  const registry = await state.bridge.getProfileRegistryRecord(profileId);
  const agentId =
    optionalString(body.agentId) ??
    optionalString(body.agent_id) ??
    registry?.agentId ??
    profileId;
  const sessionId =
    optionalString(body.sessionId) ??
    optionalString(body.session_id) ??
    `${agentId}-rp-${state
      .now()
      .replace(/[^0-9A-Za-z]/g, "")
      .slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const activeLayerIds = optionalStringArray(
    body.activeLayerIds ?? body.active_layer_ids,
    [],
    "activeLayerIds",
  );
  const session = await state.bridge.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
    resourceLimits: {},
    toolProfile: { tools: [] },
  });
  const metadata: Partial<RoleplaySessionMetadata> = {
    profileId,
    displayName:
      optionalString(body.displayName) ?? optionalString(body.display_name),
    playerPersonaId:
      optionalString(body.playerPersonaId) ??
      optionalString(body.player_persona_id),
    characterId:
      optionalString(body.characterId) ?? optionalString(body.character_id),
    activeLayerIds,
    archived: false,
    createdAt: state.now(),
  };
  await upsertRoleplaySessionMetadata(state, session.sessionId, metadata);
  if (activeLayerIds.length > 0) {
    await state.bridge.setChatLayers({
      chat_id: session.sessionId,
      layers: activeLayerIds.map((layerId, index) => ({
        layer_id: layerId,
        priority: index,
        enabled: true,
      })),
      now: state.now(),
    });
  }
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
  const patch: Partial<RoleplaySessionMetadata> = {};
  if (
    Object.hasOwn(body, "displayName") ||
    Object.hasOwn(body, "display_name")
  ) {
    patch.displayName =
      optionalString(body.displayName) ?? optionalString(body.display_name);
  }
  if (
    Object.hasOwn(body, "playerPersonaId") ||
    Object.hasOwn(body, "player_persona_id")
  ) {
    patch.playerPersonaId =
      optionalString(body.playerPersonaId) ??
      optionalString(body.player_persona_id);
  }
  if (
    Object.hasOwn(body, "characterId") ||
    Object.hasOwn(body, "character_id")
  ) {
    patch.characterId =
      optionalString(body.characterId) ?? optionalString(body.character_id);
  }
  if (
    Object.hasOwn(body, "activeLayerIds") ||
    Object.hasOwn(body, "active_layer_ids")
  ) {
    patch.activeLayerIds = optionalStringArray(
      body.activeLayerIds ?? body.active_layer_ids,
      [],
      "activeLayerIds",
    );
    await state.bridge.setChatLayers({
      chat_id: sessionId,
      layers: patch.activeLayerIds.map((layerId, index) => ({
        layer_id: layerId,
        priority: index,
        enabled: true,
      })),
      now: state.now(),
    });
  }
  await upsertRoleplaySessionMetadata(state, sessionId, patch);
  const summary = await getRoleplaySessionSummary(state, sessionId);
  if (summary === undefined)
    throw new Error(`roleplay session ${sessionId} missing`);
  return summary;
}

async function archiveRoleplaySession(
  state: RoleplayRouteContext,
  sessionId: string,
): Promise<Record<string, unknown>> {
  await state.bridge.archiveSession(sessionId as SessionId);
  await upsertRoleplaySessionMetadata(state, sessionId, { archived: true });
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
  await state.bridge.ensureConfiguredSession({
    sessionId,
    agentId: existing.agentId,
    profileId: existing.profileId,
    kind: existing.kind as "full" | "worker" | "delegated",
    resourceLimits: compactRecord(
      existing.resourceLimits as unknown as Record<string, unknown>,
    ),
    toolProfile: existing.toolProfile,
    historyWindow: existing.historyWindow,
  });
  await upsertRoleplaySessionMetadata(state, sessionId, { archived: false });
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
  const targetMessageId = requiredRouteString(
    optionalString(body.messageId) ?? optionalString(body.message_id),
    "messageId",
  );
  const targetSessionId =
    optionalString(body.sessionId) ??
    optionalString(body.session_id) ??
    optionalString(body.newSessionId) ??
    optionalString(body.new_session_id) ??
    `${sourceSession.agentId}-fork-${state
      .now()
      .replace(/[^0-9A-Za-z]/g, "")
      .slice(0, 17)}-${randomBytes(3).toString("hex")}`;
  const sourceSlots = await canonicalRoleplaySlotsThroughMessage(
    state,
    sourceSessionId,
    targetMessageId,
  );
  const targetSession = await state.bridge.createSession({
    sessionId: targetSessionId,
    agentId: sourceSession.agentId,
    profileId: sourceSession.profileId,
    kind: sourceSession.kind as "full" | "worker" | "delegated",
    resourceLimits: compactRecord(
      sourceSession.resourceLimits as unknown as Record<string, unknown>,
    ),
    toolProfile: sourceSession.toolProfile,
    historyWindow: sourceSession.historyWindow,
  });
  const sourceMetadata = await roleplaySessionMetadata(state, sourceSession);
  await upsertRoleplaySessionMetadata(state, targetSession.sessionId, {
    ...sourceMetadata,
    sessionId: targetSession.sessionId,
    profileId: targetSession.profileId,
    displayName:
      optionalString(body.displayName) ??
      optionalString(body.display_name) ??
      `${sourceMetadata.displayName ?? sourceSession.sessionId} fork`,
    archived: false,
    createdAt: state.now(),
    updatedAt: state.now(),
  });
  const sourceLayers = await state.bridge.getChatLayers(sourceSessionId);
  if (sourceLayers.length > 0) {
    await state.bridge.setChatLayers({
      chat_id: targetSession.sessionId,
      layers: sourceLayers.map((layer) => ({
        layer_id: String(layer.layer_id),
        priority: numberValue(layer.priority),
        enabled: layer.enabled !== false,
      })),
      now: state.now(),
    });
  }
  const now = state.now();
  const branchId = stableRoleplayRecordId(
    "branch",
    `${targetSession.sessionId}:fork:${targetMessageId}`,
  );
  const branch = (await state.bridge.saveConversationBranch({
    branch_id: branchId,
    session_id: targetSession.sessionId,
    parent_branch_id: null,
    parent_message_id: null,
    origin_message_id: null,
    head_message_id: null,
    label:
      optionalString(body.label) ??
      optionalString(body.branchLabel) ??
      optionalString(body.branch_label) ??
      "Fork",
    metadata_json: {
      source: "roleplay_session_fork",
      source_session_id: sourceSessionId,
      source_message_id: targetMessageId,
    },
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
    if (sourceMessage.message_id === targetMessageId) {
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
        branchId,
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
    branch_id: branchId,
    head_message_id:
      targetCopiedMessageId ?? [...copiedMessages.values()].at(-1),
    expected: { type: "any" },
    updated_at: state.now(),
  });
  await state.bridge.selectActiveConversationBranch({
    session_id: targetSession.sessionId,
    active_branch_id: branchId,
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
    source_message_id: targetMessageId,
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
  );
  const now = state.now();
  const session = await state.serviceSessionById(sessionId);
  const speakerIdentity = await roleplaySpeakerIdentitySnapshotForMessage(
    state,
    session,
    { id: "roleplay-assistant", kind: "agent" },
    now,
  ).catch(() => undefined);
  const variantId =
    optionalString(body.variantId) ??
    optionalString(body.variant_id) ??
    stableRoleplayRecordId(
      "variant",
      `${plan.terminal_slot.slot_id}:${requestIdValue}`,
    );
  const messageId =
    optionalString(body.messageId) ??
    optionalString(body.message_id) ??
    stableRoleplayRecordId("message", variantId);
  const bodyText = requiredRouteString(
    optionalString(body.body) ?? optionalString(body.text),
    "body",
  );
  const variant = (await state.bridge.saveMessageVariant(
    roleplayMessageVariantWrite({
      sessionId,
      slotId: plan.terminal_slot.slot_id,
      variantId,
      messageId,
      source: "alternate",
      ordinal: plan.next_alternate_ordinal,
      actor: { id: "roleplay-assistant", kind: "agent" },
      body: bodyText,
      branchId: plan.branch_id_for_variant ?? undefined,
      parentMessageId: plan.parent_message_id ?? undefined,
      previousMessageId: plan.previous_message_id ?? undefined,
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
  );
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
  const variantId =
    optionalString(body.variantId) ??
    optionalString(body.variant_id) ??
    stableRoleplayRecordId(
      "variant",
      `${plan.terminal_slot.slot_id}:${requestIdValue}`,
    );
  const messageId =
    optionalString(body.messageId) ??
    optionalString(body.message_id) ??
    stableRoleplayRecordId("message", variantId);
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
  const variant = (await state.bridge.saveMessageVariant(
    roleplayMessageVariantWrite({
      sessionId,
      slotId: plan.terminal_slot.slot_id,
      variantId,
      messageId,
      source: "alternate",
      ordinal: plan.next_alternate_ordinal,
      actor: { id: "roleplay-assistant", kind: "agent" },
      body: bodyText,
      branchId: plan.branch_id_for_variant ?? undefined,
      parentMessageId: plan.parent_message_id ?? undefined,
      previousMessageId: plan.previous_message_id ?? undefined,
      metadataJson,
      now,
    }),
  )) as MessageVariantRecord;
  const selected = (await state.bridge.selectActiveMessageVariant({
    slot_id: plan.terminal_slot.slot_id,
    active_variant_id: variant.variant_id,
    expected: { type: "any" },
    updated_at: state.now(),
  })) as {
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const selectedVariant = activeVariantForSlot(selected.slot);
  if (selectedVariant.message.branch_id) {
    await state.bridge.updateConversationBranchHead({
      branch_id: selectedVariant.message.branch_id,
      head_message_id: selectedVariant.message.message_id,
      expected: { type: "any" },
      updated_at: state.now(),
    });
  }
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
  const result = (await state.bridge.selectActiveMessageVariant({
    slot_id: plan.terminal_slot.slot_id,
    active_variant_id: activeVariantId ?? null,
    expected: { type: "any" },
    updated_at: state.now(),
  })) as {
    slot: MessageSlotRecord;
    conflict?: { expected?: string | null; actual?: string | null } | null;
  };
  const status = result.conflict ? "conflict" : "selected";
  if (status === "selected") {
    const selected = activeVariantForSlot(result.slot);
    if (selected?.message.branch_id) {
      await state.bridge.updateConversationBranchHead({
        branch_id: selected.message.branch_id,
        head_message_id: selected.message.message_id,
        expected: { type: "any" },
        updated_at: state.now(),
      });
    }
  }
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
    slots,
    active_branch_id: branchState?.active_branch_id ?? null,
    branches,
  })) as RoleplayAssistantAlternativePlan;
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
  return normalizeRoleplayNarratorConfig(profile.roleplayNarrator ?? {});
}

async function writeRoleplayNarratorConfig(
  state: RoleplayRouteContext,
  profileId: string,
  body: Record<string, unknown>,
): Promise<BrowserRoleplayNarratorConfig> {
  const config = normalizeRoleplayNarratorConfig(body.config ?? body);
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
  return config;
}

function normalizeRoleplayNarratorConfig(
  input: unknown,
): BrowserRoleplayNarratorConfig {
  const raw = recordBody(input);
  const review = optionalRecord(raw.review) ?? {};
  const maxReviewCycles =
    optionalNumber(review.maxReviewCycles ?? review.max_review_cycles) ?? 1;
  if (
    !Number.isInteger(maxReviewCycles) ||
    maxReviewCycles < 0 ||
    maxReviewCycles > 8
  ) {
    throw new Error(
      "review.maxReviewCycles must be an integer between 0 and 8",
    );
  }
  return {
    tone: enumValue(
      raw.tone,
      ["whimsical", "dramatic", "matter_of_fact", "lush", "wry"],
      "tone",
      "lush",
    ),
    pacing: enumValue(
      raw.pacing,
      ["leisurely", "balanced", "rapid", "breathless"],
      "pacing",
      "balanced",
    ),
    explicitness: enumValue(
      raw.explicitness,
      ["implied", "suggestive", "romantic", "steamy"],
      "explicitness",
      "romantic",
    ),
    memoryDepth: enumValue(
      raw.memoryDepth ?? raw.memory_depth,
      ["shallow", "medium", "deep"],
      "memoryDepth",
      "medium",
    ),
    ...(Object.hasOwn(raw, "stylePrompt") || Object.hasOwn(raw, "style_prompt")
      ? {
          stylePrompt:
            optionalString(raw.stylePrompt ?? raw.style_prompt) ?? "",
        }
      : {}),
    ...(Object.hasOwn(raw, "exemplar") || Object.hasOwn(raw, "styleExemplar")
      ? { exemplar: optionalString(raw.exemplar ?? raw.styleExemplar) ?? "" }
      : {}),
    review: {
      enabled: optionalBoolean(review.enabled) ?? false,
      maxReviewCycles,
    },
  };
}

function enumValue<T extends string>(
  value: unknown,
  allowed: readonly T[],
  fieldName: string,
  fallback: T,
): T {
  if (value === undefined) return fallback;
  if (typeof value === "string" && allowed.includes(value as T)) {
    return value as T;
  }
  throw new Error(`${fieldName} must be one of ${allowed.join(", ")}`);
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

function stringArray(value: unknown, fieldName: string): string[] {
  if (!Array.isArray(value)) throw new Error(`${fieldName} must be an array`);
  return value.map((item, index) =>
    requiredString(item, `${fieldName}[${index}]`),
  );
}

function optionalStringArray(
  value: unknown,
  fallback: string[],
  fieldName: string,
): string[] {
  if (value === undefined) return fallback;
  return stringArray(value, fieldName);
}

function requiredString(value: unknown, fieldName: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${fieldName} is required`);
  }
  return value;
}

function optionalBoolean(value: unknown): boolean | undefined {
  return typeof value === "boolean" ? value : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : undefined;
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
