import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { randomUUID } from "node:crypto";
import { Type, type Static } from "typebox";
import { parse as parseYaml } from "yaml";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { ProfileConfig } from "./profile-loading.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export interface RoleplayMechanicToolDetails {
  ok: boolean;
  operation:
    | "get_mechanic_capabilities"
    | "inspect_roleplay_transcript"
    | "inspect_roleplay_scene"
    | "inspect_lore_retrieval"
    | "inspect_roleplay_proposals"
    | "propose_roleplay_change"
    | "record_roleplay_diagnostic";
  action: "read" | "proposed" | "recorded" | "denied" | "failed";
  reasonCode?: string;
  result?: unknown;
}

interface RoleplayMechanicProfilePlan {
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
  localToolProfileId: string;
}

const noParameters = Type.Object({}, { additionalProperties: false });
const sessionReadParameters = Type.Object(
  {
    sessionId: Type.String({ minLength: 1 }),
    limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 100 })),
  },
  { additionalProperties: false },
);
const proposalMarkdownParameters = Type.Object(
  {
    proposal: Type.String({ minLength: 1, maxLength: 200_000 }),
  },
  { additionalProperties: false },
);
const diagnosticMarkdownParameters = Type.Object(
  {
    report: Type.String({ minLength: 1, maxLength: 200_000 }),
  },
  { additionalProperties: false },
);

type MechanicBridge = Pick<
  NativeBridgeModule,
  | "planRoleplayMechanicProfile"
  | "getRoleplaySessionMetadata"
  | "queryMessageSlots"
  | "listSimpleKv"
  | "readRoleplaySceneState"
  | "getProfileRegistryRecord"
  | "getChatLayers"
  | "listRecallTraces"
  | "createRoleplayMechanicProposal"
  | "listRoleplayMechanicProposals"
  | "getRoleplayMechanicSessionAssociation"
  | "createRoleplayMechanicDiagnostic"
>;

export function createRoleplayMechanicToolResolver(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
}): BrainToolResolver {
  return ({ wake }) => {
    const mechanicSessionId = wake.state.session.sessionId;
    return [
      getMechanicCapabilitiesTool(input),
      inspectRoleplayTranscriptTool(input),
      inspectRoleplaySceneTool(input),
      inspectLoreRetrievalTool(input),
      inspectRoleplayProposalsTool(input),
      proposeRoleplayChangeTool({ ...input, mechanicSessionId }),
      recordRoleplayDiagnosticTool({ ...input, mechanicSessionId }),
    ];
  };
}

export function recordRoleplayDiagnosticTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
  mechanicSessionId: string;
}): BrainTool<
  typeof diagnosticMarkdownParameters,
  RoleplayMechanicToolDetails
> {
  return {
    name: "record_roleplay_diagnostic",
    label: "Record roleplay diagnostic",
    description:
      "Record a pending diagnostic for this mechanic conversation from one Markdown document. Use YAML front matter with symptom, hypothesis, and optional proposal_ids and applied_proposal_ids; put supporting notes in the body.",
    parameters: diagnosticMarkdownParameters,
    executionMode: "sequential",
    execute: async (_callId, params) => {
      if (input.profile.roleplayMechanic === undefined) {
        return mechanicResultFor("record_roleplay_diagnostic", "denied", {
          ok: false,
          reasonCode: "roleplay_mechanic_profile_required",
        });
      }
      if (!input.bridge) {
        return mechanicResultFor("record_roleplay_diagnostic", "failed", {
          ok: false,
          reasonCode: "roleplay_mechanic_bridge_unavailable",
        });
      }
      try {
        const association =
          (await input.bridge.getRoleplayMechanicSessionAssociation(
            input.mechanicSessionId,
          )) as { roleplaySessionId?: string } | undefined;
        if (association?.roleplaySessionId === undefined) {
          throw new Error(
            "roleplay_mechanic_session_not_attached: attach the mechanic session before recording diagnostics",
          );
        }
        const report = parseRoleplayDiagnosticMarkdown(params.report);
        const diagnostic = await input.bridge.createRoleplayMechanicDiagnostic({
          diagnosticId: `mechanic-diagnostic:${randomUUID()}`,
          mechanicSessionId: input.mechanicSessionId,
          roleplaySessionId: association.roleplaySessionId,
          symptom: report.symptom,
          hypothesis: report.hypothesis,
          proposalIds: report.proposalIds,
          appliedProposalIds: report.appliedProposalIds,
          notes: report.notes,
          now: new Date().toISOString(),
        });
        return mechanicResultFor("record_roleplay_diagnostic", "recorded", {
          ok: true,
          result: diagnostic,
        });
      } catch (error) {
        return mechanicResultFor("record_roleplay_diagnostic", "failed", {
          ok: false,
          reasonCode: errorReasonCode(error),
          result: { message: errorMessage(error) },
        });
      }
    },
  };
}

export function getMechanicCapabilitiesTool(input: {
  bridge?: Pick<NativeBridgeModule, "planRoleplayMechanicProfile">;
  profile: ProfileConfig;
}): BrainTool<typeof noParameters, RoleplayMechanicToolDetails> {
  return {
    name: "get_mechanic_capabilities",
    label: "Get mechanic capabilities",
    description:
      "Read this mechanic profile's identity, provider selection, proposal-only mutation policy, and auto-monitor availability.",
    parameters: noParameters,
    executionMode: "parallel",
    execute: async () => {
      if (input.profile.roleplayMechanic === undefined) {
        return mechanicResult("denied", {
          ok: false,
          reasonCode: "roleplay_mechanic_profile_required",
        });
      }
      if (input.bridge === undefined) {
        return mechanicResult("failed", {
          ok: false,
          reasonCode: "roleplay_mechanic_bridge_unavailable",
        });
      }
      try {
        const plan = (await input.bridge.planRoleplayMechanicProfile({
          name: input.profile.displayName ?? input.profile.profileId,
          providerAlias: input.profile.providerAlias,
          autoMonitor: input.profile.roleplayMechanic.autoMonitor,
        })) as RoleplayMechanicProfilePlan;
        return mechanicResult("read", {
          ok: true,
          result: {
            config: plan.config,
            localToolProfileId: plan.localToolProfileId,
            mutationPolicy: "proposal_only",
            directStateWrites: false,
          },
        });
      } catch (error) {
        return mechanicResult("failed", {
          ok: false,
          reasonCode: errorReasonCode(error),
          result: { message: errorMessage(error) },
        });
      }
    },
  };
}

export function inspectRoleplayTranscriptTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
}): BrainTool<typeof sessionReadParameters, RoleplayMechanicToolDetails> {
  return mechanicReadTool(
    "inspect_roleplay_transcript",
    "Inspect roleplay transcript",
    "Read recent selected user and assistant message variants for one roleplay session, including speaker and variant provenance.",
    input,
    async (bridge, params) => {
      await requireRoleplaySession(bridge, params.sessionId);
      const slots = (await bridge.queryMessageSlots({
        session_id: params.sessionId,
        include_alternates: true,
        page: { limit: params.limit ?? 30, offset: 0 },
      })) as MessageSlot[];
      return {
        sessionId: params.sessionId,
        messages: slots
          .map(selectedTranscriptMessage)
          .filter((message): message is NonNullable<typeof message> =>
            Boolean(message),
          ),
      };
    },
  );
}

export function inspectRoleplaySceneTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
}): BrainTool<typeof sessionReadParameters, RoleplayMechanicToolDetails> {
  return mechanicReadTool(
    "inspect_roleplay_scene",
    "Inspect roleplay scene",
    "Read roleplay session ownership, active lore layers, current scene state, latest narrator scene brief, and narrator profile configuration.",
    input,
    async (bridge, params) => {
      const metadata = await requireRoleplaySession(bridge, params.sessionId);
      const [sceneRecord] = await bridge.listSimpleKv({
        scopeType: "roleplay_scene_state",
        scopeId: params.sessionId,
        keyPrefix: "current",
        now: new Date().toISOString(),
        limit: 1,
      });
      const scene = sceneRecord
        ? await bridge.readRoleplaySceneState({
            session_id: params.sessionId,
            record_value_json: sceneRecord.valueJson,
            record_updated_at: sceneRecord.updatedAt,
            revision: sceneRecord.revision,
          })
        : undefined;
      const profile = await bridge.getProfileRegistryRecord(metadata.profileId);
      const activeSettings = isRecord(profile?.activeRuntimeSettingsJson)
        ? profile.activeRuntimeSettingsJson
        : undefined;
      return {
        sessionId: params.sessionId,
        profileId: metadata.profileId,
        sceneState: scene
          ? { status: "available", ...asRecord(scene) }
          : { status: "missing" },
        narratorDiagnostic: metadata.narratorDiagnostic
          ? { status: "available", ...asRecord(metadata.narratorDiagnostic) }
          : { status: "missing" },
        activeLayers: await bridge.getChatLayers(params.sessionId),
        narratorProfile: profile
          ? {
              status: "available",
              profileId: profile.profileId,
              displayName: profile.displayName,
              providerAlias: activeSettings?.providerAlias,
              narratorConfig: activeSettings?.roleplayNarrator,
            }
          : { status: "missing" },
      };
    },
  );
}

export function inspectLoreRetrievalTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
}): BrainTool<typeof sessionReadParameters, RoleplayMechanicToolDetails> {
  return mechanicReadTool(
    "inspect_lore_retrieval",
    "Inspect lore retrieval",
    "Read recent lore-recall traces for one roleplay session, including candidate scores, inclusion decisions, budgets, and configuration snapshots.",
    input,
    async (bridge, params) => {
      await requireRoleplaySession(bridge, params.sessionId);
      const traces = await bridge.listRecallTraces({
        session_id: params.sessionId,
        page: { limit: params.limit ?? 10, offset: 0 },
      });
      return {
        sessionId: params.sessionId,
        traces: traces.length > 0 ? traces : [],
        status: traces.length > 0 ? "available" : "missing",
      };
    },
  );
}

export function inspectRoleplayProposalsTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
}): BrainTool<typeof sessionReadParameters, RoleplayMechanicToolDetails> {
  return mechanicReadTool(
    "inspect_roleplay_proposals",
    "Inspect roleplay proposals",
    "Read prior mechanic proposal outcomes and audit history for one roleplay session.",
    input,
    async (bridge, params) => {
      await requireRoleplaySession(bridge, params.sessionId);
      const proposals = await bridge.listRoleplayMechanicProposals({
        roleplaySessionId: params.sessionId,
        page: { limit: params.limit ?? 20, offset: 0 },
      });
      return {
        sessionId: params.sessionId,
        status: proposals.length > 0 ? "available" : "missing",
        proposals,
      };
    },
  );
}

export function proposeRoleplayChangeTool(input: {
  bridge?: MechanicBridge;
  profile: ProfileConfig;
  mechanicSessionId: string;
}): BrainTool<typeof proposalMarkdownParameters, RoleplayMechanicToolDetails> {
  return {
    name: "propose_roleplay_change",
    label: "Propose roleplay change",
    description:
      "Create a reviewable roleplay change proposal from one Markdown document. Use YAML front matter with roleplay_session_id, change_kind, optional target_id, and optional evidence; put the proposed value in the body. This never applies the change.",
    parameters: proposalMarkdownParameters,
    executionMode: "sequential",
    execute: async (_callId, params) => {
      if (input.profile.roleplayMechanic === undefined) {
        return mechanicResultFor("propose_roleplay_change", "denied", {
          ok: false,
          reasonCode: "roleplay_mechanic_profile_required",
        });
      }
      if (!input.bridge) {
        return mechanicResultFor("propose_roleplay_change", "failed", {
          ok: false,
          reasonCode: "roleplay_mechanic_bridge_unavailable",
        });
      }
      try {
        const draft = parseRoleplayProposalMarkdown(params.proposal);
        const proposal = await input.bridge.createRoleplayMechanicProposal({
          proposalId: `mechanic-proposal:${randomUUID()}`,
          mechanicSessionId: input.mechanicSessionId,
          roleplaySessionId: draft.roleplaySessionId,
          kind: draft.kind,
          targetId: draft.targetId,
          proposedValue: draft.proposedValue,
          rationale: draft.rationale,
          diagnosticContext: draft.diagnosticContext,
          now: new Date().toISOString(),
        });
        return mechanicResultFor("propose_roleplay_change", "proposed", {
          ok: true,
          result: proposal,
        });
      } catch (error) {
        return mechanicResultFor("propose_roleplay_change", "failed", {
          ok: false,
          reasonCode: errorReasonCode(error),
          result: { message: errorMessage(error) },
        });
      }
    },
  };
}

function mechanicReadTool(
  operation: Exclude<
    RoleplayMechanicToolDetails["operation"],
    "get_mechanic_capabilities"
  >,
  label: string,
  description: string,
  input: { bridge?: MechanicBridge; profile: ProfileConfig },
  read: (
    bridge: MechanicBridge,
    params: Static<typeof sessionReadParameters>,
  ) => Promise<unknown>,
): BrainTool<typeof sessionReadParameters, RoleplayMechanicToolDetails> {
  return {
    name: operation,
    label,
    description,
    parameters: sessionReadParameters,
    executionMode: "parallel",
    execute: async (_callId, params) => {
      if (input.profile.roleplayMechanic === undefined) {
        return mechanicResultFor(operation, "denied", {
          ok: false,
          reasonCode: "roleplay_mechanic_profile_required",
        });
      }
      if (!input.bridge) {
        return mechanicResultFor(operation, "failed", {
          ok: false,
          reasonCode: "roleplay_mechanic_bridge_unavailable",
        });
      }
      try {
        return mechanicResultFor(operation, "read", {
          ok: true,
          result: await read(input.bridge, params),
        });
      } catch (error) {
        return mechanicResultFor(operation, "failed", {
          ok: false,
          reasonCode: errorReasonCode(error),
          result: { message: errorMessage(error) },
        });
      }
    },
  };
}

interface RoleplaySessionMetadata {
  sessionId: string;
  profileId: string;
  narratorDiagnostic?: unknown;
}

interface MessageSlot {
  slot_id: string;
  active_variant_id?: string | null;
  primary: MessageVariant;
  alternates?: MessageVariant[];
}

interface MessageVariant {
  variant_id: string;
  source: string;
  message: {
    author_id: string;
    author_role: string;
    body: string;
    created_at: string;
  };
}

interface ParsedRoleplayProposalDraft {
  roleplaySessionId: string;
  kind:
    | "narrator_config"
    | "exemplar"
    | "lore_add"
    | "lore_edit"
    | "lore_tags"
    | "layer_retrieval_config"
    | "provider_failure_pattern";
  targetId?: string;
  proposedValue: unknown;
  rationale: string;
  diagnosticContext: unknown;
}

const roleplayProposalKinds = new Set<ParsedRoleplayProposalDraft["kind"]>([
  "narrator_config",
  "exemplar",
  "lore_add",
  "lore_edit",
  "lore_tags",
  "layer_retrieval_config",
  "provider_failure_pattern",
]);

interface ParsedRoleplayDiagnosticDraft {
  symptom: string;
  hypothesis: string;
  proposalIds: string[];
  appliedProposalIds: string[];
  notes?: string;
}

function parseRoleplayDiagnosticMarkdown(
  markdown: string,
): ParsedRoleplayDiagnosticDraft {
  const match = markdown.match(
    /^---\s*\r?\n([\s\S]*?)\r?\n---(?:\s*\r?\n([\s\S]*))?$/,
  );
  if (!match) {
    throw new Error(
      "roleplay_mechanic_diagnostic_front_matter_required: report must begin with YAML front matter",
    );
  }
  const header = parseYaml(match[1] ?? "");
  if (!isRecord(header)) {
    throw new Error(
      "roleplay_mechanic_diagnostic_front_matter_invalid: front matter must be a YAML mapping",
    );
  }
  return {
    symptom: requiredHeaderString(header, "symptom"),
    hypothesis: requiredHeaderString(header, "hypothesis"),
    proposalIds: optionalHeaderStringArray(header, "proposal_ids"),
    appliedProposalIds: optionalHeaderStringArray(
      header,
      "applied_proposal_ids",
    ),
    notes: optionalBody(match[2]),
  };
}

function parseRoleplayProposalMarkdown(
  markdown: string,
): ParsedRoleplayProposalDraft {
  const match = markdown.match(
    /^---\s*\r?\n([\s\S]*?)\r?\n---\s*\r?\n([\s\S]*)$/,
  );
  if (!match) {
    throw new Error(
      "roleplay_mechanic_proposal_front_matter_required: proposal must begin with YAML front matter",
    );
  }
  const header = parseYaml(match[1] ?? "");
  if (!isRecord(header)) {
    throw new Error(
      "roleplay_mechanic_proposal_front_matter_invalid: front matter must be a YAML mapping",
    );
  }
  const roleplaySessionId = requiredHeaderString(header, "roleplay_session_id");
  const rawKind = requiredHeaderString(header, "change_kind");
  if (
    !roleplayProposalKinds.has(rawKind as ParsedRoleplayProposalDraft["kind"])
  ) {
    throw new Error(
      `roleplay_mechanic_proposal_kind_invalid: unsupported change_kind ${rawKind}`,
    );
  }
  const kind = rawKind as ParsedRoleplayProposalDraft["kind"];
  const rationale = requiredHeaderString(header, "rationale");
  const targetId = optionalHeaderString(header, "target_id");
  const body = (match[2] ?? "").trim();
  if (!body) {
    throw new Error(
      "roleplay_mechanic_proposal_value_required: proposal body must contain the proposed value",
    );
  }

  const proposedValue = kind === "exemplar" ? body : parseYaml(body);
  if (proposedValue === undefined) {
    throw new Error(
      "roleplay_mechanic_proposal_value_invalid: proposal body must contain valid YAML",
    );
  }
  const diagnosticContext =
    header.diagnostic_context !== undefined
      ? header.diagnostic_context
      : header.evidence !== undefined
        ? { evidence: header.evidence }
        : {};

  return {
    roleplaySessionId,
    kind,
    targetId,
    proposedValue,
    rationale,
    diagnosticContext,
  };
}

function requiredHeaderString(
  header: Record<string, unknown>,
  key: string,
): string {
  const value = optionalHeaderString(header, key);
  if (!value) {
    throw new Error(
      `roleplay_mechanic_proposal_${key}_required: front matter requires ${key}`,
    );
  }
  return value;
}

function optionalHeaderString(
  header: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = header[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(
      `roleplay_mechanic_proposal_${key}_invalid: ${key} must be non-empty text`,
    );
  }
  return value.trim();
}

function optionalHeaderStringArray(
  header: Record<string, unknown>,
  key: string,
): string[] {
  const value = header[key];
  if (value === undefined || value === null) return [];
  if (
    !Array.isArray(value) ||
    value.some((item) => typeof item !== "string" || item.trim() === "")
  ) {
    throw new Error(
      `roleplay_mechanic_diagnostic_${key}_invalid: ${key} must be a list of non-empty strings`,
    );
  }
  return value.map((item) => (item as string).trim());
}

function optionalBody(value: string | undefined): string | undefined {
  const body = value?.trim();
  return body ? body : undefined;
}

async function requireRoleplaySession(
  bridge: MechanicBridge,
  sessionId: string,
): Promise<RoleplaySessionMetadata> {
  const metadata = (await bridge.getRoleplaySessionMetadata(sessionId)) as
    | RoleplaySessionMetadata
    | undefined;
  if (!metadata) {
    throw new Error(`roleplay_session_not_found: ${sessionId}`);
  }
  return metadata;
}

function selectedTranscriptMessage(slot: MessageSlot) {
  const selected = slot.active_variant_id
    ? (slot.alternates?.find(
        (variant) => variant.variant_id === slot.active_variant_id,
      ) ?? slot.primary)
    : slot.primary;
  const role = selected.message.author_role;
  if (
    !(["user", "assistant"] as const).includes(role as "user" | "assistant")
  ) {
    return undefined;
  }
  if (!selected.message.body.trim()) return undefined;
  return {
    slotId: slot.slot_id,
    variantId: selected.variant_id,
    variantSource: selected.source,
    authorId: selected.message.author_id,
    role,
    body: selected.message.body,
    createdAt: selected.message.created_at,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function asRecord(value: unknown): Record<string, unknown> {
  return isRecord(value) ? value : { value };
}

function mechanicResult(
  action: RoleplayMechanicToolDetails["action"],
  detail: Omit<RoleplayMechanicToolDetails, "operation" | "action">,
): BrainToolResult<RoleplayMechanicToolDetails> {
  const details: RoleplayMechanicToolDetails = {
    operation: "get_mechanic_capabilities",
    action,
    ...detail,
  };
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function mechanicResultFor(
  operation: RoleplayMechanicToolDetails["operation"],
  action: RoleplayMechanicToolDetails["action"],
  detail: Omit<RoleplayMechanicToolDetails, "operation" | "action">,
): BrainToolResult<RoleplayMechanicToolDetails> {
  const details: RoleplayMechanicToolDetails = { operation, action, ...detail };
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function errorReasonCode(error: unknown): string {
  const message = errorMessage(error);
  return (
    message.match(/\b(roleplay_[a-z0-9_]+)\b/)?.[1] ??
    "roleplay_mechanic_capabilities_failed"
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
