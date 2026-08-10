import type { AdminControlCommandName } from "./admin-control-api.js";
import { PROFILE_REGISTRY_ADMIN_PATHS } from "./profile-registry-admin-contract.js";
import { nativeReasoningEffortList } from "./reasoning-effort-policy.js";
import { RUSTY_VIEW_CHAT_PATHS } from "./rusty-view-chat-contract.js";

export type ApiCapabilityAuth = "none" | "chat" | "admin";
export type ApiCapabilityMutation = "read" | "write" | "control";
export type ApiCapabilityStability = "stable" | "experimental";
export type ChatCommandArgumentType =
  | "string"
  | "number"
  | "boolean"
  | "enum"
  | "json"
  | "file";
export type ChatCommandSurface = "chat-input" | "global" | "message-context";
export type ChatCommandSource =
  | "backend"
  | "backend-control"
  | "frontend-local"
  | "plugin";
export type ApiCapabilityScope =
  | "attachment"
  | "chat"
  | "conversation"
  | "diagnostics"
  | "profile"
  | "session"
  | "delegation"
  | "mcp"
  | "config"
  | "prompt"
  | "governance"
  | "maintenance"
  | "media"
  | "memory"
  | "scheduler"
  | "search"
  | "storage"
  | "curator"
  | "tool"
  | "service";

export interface SlashCommandDefinition<Name extends string = string> {
  name: Name;
  aliases: readonly string[];
  description: string;
  argsSchema: Record<string, unknown>;
  positionalArgs: readonly ChatCommandArgumentDescriptor[];
  namedArgs: readonly ChatCommandArgumentDescriptor[];
  surfaces: readonly ChatCommandSurface[];
  source: ChatCommandSource;
  readOnly: boolean;
  mutating: boolean;
  scope: "session" | "profile" | "service";
  allowedSessionKinds: readonly ["full", ...Array<"worker" | "delegated">];
  requiresControlAuth: boolean;
  control?: {
    commandName: AdminControlCommandName;
    pathTemplate: string;
    reasonCode: string;
    rustPlanOperation?: string;
  };
}

export type SlashCommandDescriptor = SlashCommandDefinition<SlashCommandName>;

export interface ChatCommandRegistry {
  commands: ChatCommandDescriptor[];
}

export interface ChatCommandDescriptor {
  name: string;
  aliases: string[];
  description: string;
  args_schema: Record<string, unknown>;
  positional_args: ChatCommandArgumentDescriptor[];
  named_args: ChatCommandArgumentDescriptor[];
  surfaces: ChatCommandSurface[];
  source: ChatCommandSource;
  read_only: boolean;
  mutating: boolean;
  scope: "session" | "profile" | "service";
  allowed_session_kinds: Array<"full" | "worker" | "delegated">;
  requires_control_auth: boolean;
  backing_control_command?: AdminControlCommandName;
  rust_plan_operation?: string;
}

export interface ChatCommandArgumentDescriptor {
  name: string;
  description?: string;
  type: ChatCommandArgumentType;
  required: boolean;
  default_value?: unknown;
  enum_values?: ChatCommandEnumValue[];
  enum_provider?: string;
  repeated?: boolean;
  placeholder?: string;
}

export interface ChatCommandEnumValue {
  value: string;
  label?: string;
  description?: string;
}

export interface ChatCommandAutocompleteResult {
  command_name: string;
  argument_name: string;
  provider?: string;
  items: ChatCommandEnumValue[];
  has_more: boolean;
}

export interface ApiCapabilityDescriptor {
  id: string;
  method: "DELETE" | "GET" | "PATCH" | "POST";
  path_template: string;
  description: string;
  auth: ApiCapabilityAuth;
  mutation: ApiCapabilityMutation;
  stability: ApiCapabilityStability;
  tags: ApiCapabilityScope[];
  public: boolean;
  command_name?: AdminControlCommandName;
  rust_plan_operation?: string;
}

export interface ApiCapabilityRegistry {
  schema_version: 1;
  slash_commands: ChatCommandDescriptor[];
  capabilities: ApiCapabilityDescriptor[];
}

const OPTIONAL_ARGS_SCHEMA = {
  type: "string",
  description: "Optional command arguments.",
} satisfies Record<string, unknown>;

const OPTIONAL_REASON_ARGUMENT = {
  name: "reason",
  description: "Optional operator-facing reason text.",
  type: "string",
  required: false,
  placeholder: "reason",
} satisfies ChatCommandArgumentDescriptor;

export const SLASH_COMMAND_REGISTRY = [
  slashCommand({
    name: "help",
    description: "Show available slash commands.",
    readOnly: true,
  }),
  slashCommand({
    name: "status",
    description: "Show runtime status for this service.",
    readOnly: true,
  }),
  slashCommand({
    name: "session",
    description: "Show details for the current session.",
    readOnly: true,
  }),
  slashCommand({
    name: "model",
    description:
      "Show the active model provider, brain backend, and context estimate.",
    readOnly: true,
  }),
  slashCommand({
    name: "effort",
    description:
      "Show or override reasoning effort for the current session; use default to clear the override.",
    readOnly: false,
    positionalArgs: [
      {
        name: "effort",
        description: `One of ${nativeReasoningEffortList()}, or default.`,
        type: "string",
        required: false,
        placeholder: "default|none|minimal|low|medium|high|xhigh",
      },
    ],
    control: {
      commandName: "set_session_effort",
      pathTemplate: "/v1/admin/control/sessions/{session_id}/effort",
      reasonCode: "slash_set_session_effort",
    },
  }),
  slashCommand({
    name: "archive",
    description: "Archive the current Crew brain session without replacing it.",
    readOnly: false,
    positionalArgs: [OPTIONAL_REASON_ARGUMENT],
    control: {
      commandName: "archive_session",
      pathTemplate: "/v1/admin/control/sessions/{session_id}/archive",
      reasonCode: "slash_archive_session",
    },
  }),
  slashCommand({
    name: "new",
    description:
      "Archive the current session and create a fresh replacement session.",
    readOnly: false,
    positionalArgs: [OPTIONAL_REASON_ARGUMENT],
    control: {
      commandName: "new_session",
      pathTemplate: "/v1/admin/control/sessions/{session_id}/new",
      reasonCode: "slash_new_session",
      rustPlanOperation: "plan_new_session_control",
    },
  }),
  slashCommand({
    name: "reload-mcp",
    description: "Reload MCP tools for the current session profile surface.",
    readOnly: false,
    control: {
      commandName: "reload_mcp",
      pathTemplate: "/v1/admin/control/mcp/{session_id}/reload",
      reasonCode: "slash_reload_mcp",
      rustPlanOperation: "plan_reload_mcp_control",
    },
  }),
] as const;

export type SlashCommandName = (typeof SLASH_COMMAND_REGISTRY)[number]["name"];

export const ADMIN_CONTROL_CAPABILITIES = [
  controlCapability(
    "admin.control.profiles.create",
    "POST",
    "/v1/admin/control/profiles",
    "Create a profile and its initial session; full sessions require an explicit workspaceCwd.",
    "create_profile",
    ["profile"],
  ),
  controlCapability(
    "admin.control.profiles.read",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/read",
    "Read backend-owned editable profile configuration for a profile.",
    "read_profile_config",
    ["profile"],
  ),
  controlCapability(
    "admin.control.profiles.update.plan",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/update/plan",
    "Validate and plan a profile file update without writing it.",
    "plan_profile_update",
    ["profile", "config"],
  ),
  controlCapability(
    "admin.control.profiles.update.apply",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/update/apply",
    "Apply a validated profile file update and reload service configuration.",
    "apply_profile_update",
    ["profile", "config"],
  ),
  controlCapability(
    "admin.control.profiles.decommission",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/decommission",
    "Decommission a profile by removing service plumbing and archiving active sessions while preserving profile files.",
    "decommission_profile",
    ["profile", "session", "config"],
  ),
  controlCapability(
    "admin.control.profiles.delete",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/delete",
    "Hard-delete a profile by removing service plumbing, profile files, registry state, sessions, and profile-owned persisted data. Requires confirmProfileId in the request body.",
    "delete_profile",
    ["profile", "session", "config", "storage"],
  ),
  controlCapability(
    "admin.control.sessions.create",
    "POST",
    "/v1/admin/control/sessions",
    "Create a runtime session with an explicit absolute workspaceCwd and optional bounded resource limits.",
    "create_session",
    ["session"],
  ),
  controlCapability(
    "admin.control.sessions.workspace",
    "POST",
    "/v1/admin/control/sessions/{session_id}/workspace",
    "Switch an idle session to an absolute workspace cwd using its expected workspace revision.",
    "switch_session_workspace",
    ["session"],
  ),
  controlCapability(
    "admin.control.sessions.archive",
    "POST",
    "/v1/admin/control/sessions/{session_id}/archive",
    "Archive a runtime session.",
    "archive_session",
    ["session"],
  ),
  controlCapability(
    "admin.control.sessions.new",
    "POST",
    "/v1/admin/control/sessions/{session_id}/new",
    "Archive a session and create a fresh replacement.",
    "new_session",
    ["session"],
    { rustPlanOperation: "plan_new_session_control" },
  ),
  controlCapability(
    "admin.control.sessions.effort",
    "POST",
    "/v1/admin/control/sessions/{session_id}/effort",
    "Set or clear the reasoning-effort override for one durable session.",
    "set_session_effort",
    ["session"],
  ),
  controlCapability(
    "admin.control.sessions.runtime.pause",
    "POST",
    "/v1/admin/control/sessions/{session_id}/runtime/pause",
    "Pause runtime work for one session without archiving its durable records.",
    "pause_runtime",
    ["session", "service"],
  ),
  controlCapability(
    "admin.control.sessions.runtime.resume",
    "POST",
    "/v1/admin/control/sessions/{session_id}/runtime/resume",
    "Resume runtime work for one paused session.",
    "resume_runtime",
    ["session", "service"],
  ),
  controlCapability(
    "admin.control.profiles.runtime.pause",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/runtime/pause",
    "Pause runtime work for all sessions using one profile.",
    "pause_runtime",
    ["profile", "session", "service"],
  ),
  controlCapability(
    "admin.control.profiles.runtime.resume",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/runtime/resume",
    "Resume runtime work for a paused profile.",
    "resume_runtime",
    ["profile", "session", "service"],
  ),
  controlCapability(
    "admin.control.agents.runtime.pause",
    "POST",
    "/v1/admin/control/agents/{agent_id}/runtime/pause",
    "Pause runtime work for all sessions belonging to one agent id.",
    "pause_runtime",
    ["session", "service"],
  ),
  controlCapability(
    "admin.control.agents.runtime.resume",
    "POST",
    "/v1/admin/control/agents/{agent_id}/runtime/resume",
    "Resume runtime work for a paused agent id.",
    "resume_runtime",
    ["session", "service"],
  ),
  controlCapability(
    "admin.control.sessions.rebuild_runtime.plan",
    "POST",
    "/v1/admin/control/sessions/{session_id}/rebuild-runtime/plan",
    "Plan the runtime impact of rebuilding a session brain from current profile config.",
    "plan_runtime_rebuild",
    ["session", "profile"],
  ),
  controlCapability(
    "admin.control.sessions.rebuild_runtime.apply",
    "POST",
    "/v1/admin/control/sessions/{session_id}/rebuild-runtime/apply",
    "Apply a guarded runtime rebuild for a session when the backend can preserve state safely.",
    "apply_runtime_rebuild",
    ["session", "profile"],
  ),
  controlCapability(
    "admin.control.profiles.rebuild_brain.plan",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/rebuild-brain/plan",
    "Plan the runtime impact of rebuilding all active sessions for a profile brain.",
    "plan_runtime_rebuild",
    ["profile", "session"],
  ),
  controlCapability(
    "admin.control.profiles.rebuild_brain.apply",
    "POST",
    "/v1/admin/control/profiles/{profile_id}/rebuild-brain/apply",
    "Apply a guarded profile brain rebuild when the backend can preserve sessions safely.",
    "apply_runtime_rebuild",
    ["profile", "session"],
  ),
  controlCapability(
    "admin.control.delegations.cancel",
    "POST",
    "/v1/admin/control/delegations/{session_id}/cancel",
    "Cancel a delegated session.",
    "cancel_delegation",
    ["delegation"],
  ),
  controlCapability(
    "admin.control.delegations.checkpoint",
    "POST",
    "/v1/admin/control/delegations/{session_id}/checkpoint",
    "Request a checkpoint from a delegated session.",
    "request_delegated_checkpoint",
    ["delegation"],
  ),
  controlCapability(
    "admin.control.config.reload",
    "POST",
    "/v1/admin/control/config/reload",
    "Reload service configuration.",
    "reload_config",
    ["config"],
  ),
  controlCapability(
    "admin.control.config.draft.plan",
    "POST",
    "/v1/admin/control/config/draft/plan",
    "Validate and plan a service runtime config draft without writing it.",
    "plan_runtime_config_update",
    ["config"],
  ),
  controlCapability(
    "admin.control.config.draft.apply",
    "POST",
    "/v1/admin/control/config/draft/apply",
    "Apply a validated service runtime config draft and reload runtime config.",
    "apply_runtime_config_update",
    ["config"],
  ),
  controlCapability(
    "admin.control.mcp.reload",
    "POST",
    "/v1/admin/control/mcp/{session_id}/reload",
    "Reload MCP surfaces for a session.",
    "reload_mcp",
    ["mcp", "session"],
    { rustPlanOperation: "plan_reload_mcp_control" },
  ),
  controlCapability(
    "admin.control.maintenance.run",
    "POST",
    "/v1/admin/control/maintenance",
    "Run service maintenance.",
    "run_maintenance",
    ["maintenance"],
  ),
  controlCapability(
    "admin.control.scheduler.tick",
    "POST",
    "/v1/admin/control/scheduler/tick",
    "Run one scheduler tick.",
    "scheduler_tick",
    ["scheduler"],
  ),
  controlCapability(
    "admin.control.scheduler.jobs.run",
    "POST",
    "/v1/admin/control/scheduler/jobs/{job_id}/run",
    "Run a scheduler job.",
    "scheduler_run_job",
    ["scheduler"],
  ),
  controlCapability(
    "admin.control.scheduler.jobs.pause",
    "POST",
    "/v1/admin/control/scheduler/jobs/{job_id}/pause",
    "Pause a scheduler job.",
    "scheduler_pause_job",
    ["scheduler"],
  ),
  controlCapability(
    "admin.control.scheduler.jobs.resume",
    "POST",
    "/v1/admin/control/scheduler/jobs/{job_id}/resume",
    "Resume a scheduler job.",
    "scheduler_resume_job",
    ["scheduler"],
  ),
  controlCapability(
    "admin.control.cleanup.delegated.run",
    "POST",
    "/v1/admin/control/cleanup/delegated/run",
    "Clean up expired delegated resources.",
    "cleanup_delegated_resources",
    ["maintenance", "delegation"],
  ),
  controlCapability(
    "admin.control.curator.status",
    "POST",
    "/v1/admin/control/curator/status",
    "Read curator status through the audited control path.",
    "curator_status",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.run",
    "POST",
    "/v1/admin/control/curator/run",
    "Run a curator scan.",
    "curator_run_scan",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.pinned.list",
    "POST",
    "/v1/admin/control/curator/pinned/list",
    "List pinned curator skills.",
    "curator_list_pinned_skills",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.archives.list",
    "POST",
    "/v1/admin/control/curator/archives/list",
    "List archived curator skills.",
    "curator_list_archived_skills",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.skills.pin",
    "POST",
    "/v1/admin/control/curator/skills/{slug}/pin",
    "Pin a curator skill.",
    "curator_pin_skill",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.skills.unpin",
    "POST",
    "/v1/admin/control/curator/skills/{slug}/unpin",
    "Unpin a curator skill.",
    "curator_unpin_skill",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.skills.restore",
    "POST",
    "/v1/admin/control/curator/skills/{slug}/restore",
    "Restore an archived curator skill.",
    "curator_restore_skill",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.candidates.preview",
    "POST",
    "/v1/admin/control/curator/candidates/{candidate_id}/preview",
    "Preview a curator candidate.",
    "curator_preview_candidate",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.candidates.approve",
    "POST",
    "/v1/admin/control/curator/candidates/{candidate_id}/approve",
    "Approve a curator candidate.",
    "curator_approve_candidate",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.candidates.apply",
    "POST",
    "/v1/admin/control/curator/candidates/{candidate_id}/apply",
    "Apply a curator candidate.",
    "curator_apply_candidate",
    ["curator"],
  ),
  controlCapability(
    "admin.control.curator.mutations.rollback",
    "POST",
    "/v1/admin/control/curator/mutations/{mutation_id}/rollback",
    "Roll back a curator mutation.",
    "curator_rollback_mutation",
    ["curator"],
  ),
  controlCapability(
    "admin.control.shutdown",
    "POST",
    "/v1/admin/control/shutdown",
    "Request service shutdown.",
    "shutdown",
    ["service"],
  ),
] as const satisfies readonly ApiCapabilityDescriptor[];

export const API_CAPABILITIES = [
  readCapability(
    "admin.logical_turns.list",
    "GET",
    "/v1/admin/logical-turns",
    "List Rust-owned logical-turn continuation and progress diagnostics.",
    "admin",
    ["session", "diagnostics"],
  ),
  controlApiCapability(
    "admin.logical_turns.cancel",
    "POST",
    "/v1/admin/logical-turns/{logical_turn_id}/cancel",
    "Cancel an active or yielded logical turn and its provider run.",
    ["session"],
  ),
  controlApiCapability(
    "admin.logical_turns.resolve",
    "POST",
    "/v1/admin/logical-turns/{logical_turn_id}/resolve",
    "Resolve operator attention, including unknown tool outcomes, and queue the logical turn to continue.",
    ["session"],
  ),
  readCapability(
    "chat.sessions.logical_turns.list",
    "GET",
    "/v1/chat/sessions/{session_id}/logical-turns",
    "List logical-turn continuation and progress diagnostics for one chat session.",
    "chat",
    ["chat", "session", "diagnostics"],
  ),
  mutationCapability(
    "chat.sessions.logical_turns.cancel",
    "POST",
    "/v1/chat/sessions/{session_id}/logical-turns/{logical_turn_id}/cancel",
    "Cancel an active or yielded logical turn from its chat session.",
    "chat",
    ["chat", "session"],
  ),
  mutationCapability(
    "chat.sessions.logical_turns.resolve",
    "POST",
    "/v1/chat/sessions/{session_id}/logical-turns/{logical_turn_id}/resolve",
    "Resolve operator attention for a chat logical turn, including unknown tool outcomes, and queue it to continue.",
    "chat",
    ["chat", "session"],
  ),
  writeCapability(
    "external.agent_sessions.create",
    "POST",
    "/v1/external-agent-sessions",
    "Atomically create or recover a Crew session, binding, and native Codex app-server thread.",
    "admin",
    ["session", "profile"],
  ),
  readCapability(
    "external.runtimes.list",
    "GET",
    "/v1/external-runtimes",
    "List Rust-owned external runtime registrations and live controller status.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "external.runtimes.register",
    "POST",
    "/v1/external-runtimes",
    "Register or revise an exact-version external agent runtime.",
    "admin",
    ["service", "config"],
  ),
  readCapability(
    "external.runtimes.read",
    "GET",
    "/v1/external-runtimes/{runtime_id}",
    "Read one external runtime and its controller status.",
    "admin",
    ["service", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.promotion_readiness",
    "GET",
    "/v1/admin/external-runtime-promotion-readiness",
    "Read exact external-runtime bindings, active turns, and unresolved interactions before operator promotion.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.certifications.list",
    "GET",
    "/v1/admin/external-runtime-certifications",
    "List durable external runtime compatibility certification history.",
    "admin",
    ["service", "diagnostics"],
  ),
  writeCapability(
    "external.runtimes.certifications.create",
    "POST",
    "/v1/admin/external-runtime-certifications",
    "Certify the exact identity of a ready runtime from passing probe evidence.",
    "admin",
    ["service", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.certifications.read",
    "GET",
    "/v1/admin/external-runtime-certifications/{certification_id}",
    "Read one durable external runtime compatibility certification.",
    "admin",
    ["service", "diagnostics"],
  ),
  controlApiCapability(
    "external.runtimes.certifications.invalidate",
    "POST",
    "/v1/admin/external-runtime-certifications/{certification_id}/invalidate",
    "Revision-check and invalidate one active runtime certification.",
    ["service", "diagnostics"],
  ),
  controlApiCapability(
    "external.runtimes.connect",
    "POST",
    "/v1/external-runtimes/{runtime_id}/connect",
    "Acquire the Rust controller lease and connect the external runtime.",
    ["service", "session"],
  ),
  readCapability(
    "external.runtimes.threads.list",
    "GET",
    "/v1/external-runtimes/{runtime_id}/threads",
    "List native external-runtime threads through the Crew controller.",
    "admin",
    ["session"],
  ),
  writeCapability(
    "external.runtimes.threads.read",
    "POST",
    "/v1/external-runtimes/{runtime_id}/threads/read",
    "Read one native thread through a bounded runtime-neutral envelope.",
    "admin",
    ["session"],
  ),
  controlApiCapability(
    "external.runtimes.threads.archive",
    "POST",
    "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/archive",
    "Archive native Codex history and archive every associated Crew binding.",
    ["session"],
  ),
  controlApiCapability(
    "external.runtimes.threads.delete",
    "POST",
    "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/delete",
    "Hard-delete native Codex history after archiving every associated Crew binding.",
    ["session"],
  ),
  controlApiCapability(
    "external.runtimes.threads.unarchive",
    "POST",
    "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/unarchive",
    "Restore native Codex history without implicitly reactivating Crew bindings.",
    ["session"],
  ),
  readCapability(
    "external.runtimes.events.list",
    "GET",
    "/v1/external-runtimes/{runtime_id}/events",
    "Replay normalized Rust-sequenced external runtime events.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.events.head",
    "GET",
    "/v1/external-runtimes/{runtime_id}/events/head",
    "Read the latest normalized external runtime event cursor without replaying history.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.events.stream",
    "GET",
    "/v1/external-runtimes/{runtime_id}/stream",
    "Stream normalized external runtime events with cursor replay.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "external.runtimes.raw_details.read",
    "GET",
    "/v1/external-runtimes/{runtime_id}/raw-details/{detail_id}",
    "Read one bounded redacted in-memory native protocol detail.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "external.bindings.list",
    "GET",
    "/v1/external-bindings",
    "List Crew-owned external agent bindings.",
    "admin",
    ["session", "profile"],
  ),
  writeCapability(
    "external.bindings.write",
    "POST",
    "/v1/external-bindings",
    "Create or revise a Crew-owned external agent binding.",
    "admin",
    ["session", "profile"],
  ),
  writeCapability(
    "external.bindings.metadata.write",
    "POST",
    "/v1/external-bindings/{binding_id}/metadata",
    "Revise only an external binding's operator label and Den task mapping with optimistic concurrency.",
    "admin",
    ["session", "config"],
  ),
  controlApiCapability(
    "external.bindings.restore",
    "POST",
    "/v1/external-bindings/{binding_id}/restore",
    "Restore an archived external binding and exact Crew session without replacing its native thread.",
    ["session", "profile"],
  ),
  controlApiCapability(
    "external.bindings.profile.refresh",
    "POST",
    "/v1/external-bindings/{binding_id}/profile-refresh",
    "Explicitly apply the current profile prompt to a bound Codex agent using optimistic concurrency.",
    ["session", "profile", "config"],
  ),
  controlApiCapability(
    "external.bindings.control",
    "POST",
    "/v1/external-bindings/{binding_id}/controls",
    "Submit an idempotent Rust-validated external runtime control.",
    ["session"],
  ),
  readCapability(
    "external.bindings.commands.list",
    "GET",
    "/v1/external-bindings/{binding_id}/commands",
    "List capability-gated commands, native models, and effort options for an external thread.",
    "admin",
    ["session", "config"],
  ),
  controlApiCapability(
    "external.bindings.commands.execute",
    "POST",
    "/v1/external-bindings/{binding_id}/commands",
    "Execute a durable Rust-validated command without creating an external-runtime user turn.",
    ["session", "config"],
  ),
  writeCapability(
    "external.bindings.messages.create",
    "POST",
    "/v1/external-bindings/{binding_id}/messages",
    "Send an operator-authored TTL-bound message through Rust activation or queue-next-turn policy.",
    "admin",
    ["session"],
  ),
  readCapability(
    "external.interactions.list",
    "GET",
    "/v1/external-interactions",
    "List pending external runtime interactions requiring operator attention.",
    "admin",
    ["session"],
  ),
  controlApiCapability(
    "external.interactions.resolve",
    "POST",
    "/v1/external-interactions/{interaction_id}/resolve",
    "Resolve a typed pending external runtime interaction.",
    ["session"],
  ),
  readCapability(
    "external.turns.read",
    "GET",
    "/v1/external-turns/{request_id}",
    "Read durable Crew/native external turn lifecycle state by activation request ID.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "agent.rounds.read",
    "GET",
    "/v1/agent-rounds/{round_id}",
    "Read durable cross-runtime correlated round state.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "agent.deliveries.read",
    "GET",
    "/v1/agent-deliveries/{delivery_id}",
    "Read durable cross-runtime message delivery and activation state.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "coordination.production.agents.list",
    "GET",
    "/v1/coordination/agents",
    "List recipients on the production deployment.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "coordination.production.routes.list",
    "GET",
    "/v1/coordination/routes",
    "List production switchboard routes and current exact-target resolution status.",
    "admin",
    ["service", "session", "delegation"],
  ),
  writeCapability(
    "coordination.production.routes.create",
    "POST",
    "/v1/coordination/routes",
    "Create a revisioned production switchboard route.",
    "admin",
    ["service", "session", "delegation"],
  ),
  readCapability(
    "coordination.production.routes.read",
    "GET",
    "/v1/coordination/routes/{route_key}",
    "Read one production switchboard route and current resolution status.",
    "admin",
    ["service", "session", "delegation"],
  ),
  writeCapability(
    "coordination.production.routes.update",
    "PATCH",
    "/v1/coordination/routes/{route_key}",
    "Replace one production switchboard route with optimistic revision control.",
    "admin",
    ["service", "session", "delegation"],
  ),
  writeCapability(
    "coordination.production.routes.delete",
    "DELETE",
    "/v1/coordination/routes/{route_key}",
    "Delete one production switchboard route with optimistic revision control.",
    "admin",
    ["service", "session", "delegation"],
  ),
  writeCapability(
    "coordination.production.routes.resolve",
    "POST",
    "/v1/coordination/routes/resolve",
    "Resolve an exact production address without delivering a message.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.production.routes.test",
    "POST",
    "/v1/coordination/routes/{route_key}/test",
    "Send a bounded test delivery through one production switchboard route.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.production.messages.create",
    "POST",
    "/v1/coordination/messages",
    "Send a TTL-bound operator message on the production deployment.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "coordination.production.messages.list",
    "GET",
    "/v1/coordination/messages",
    "Inspect every durable production delivery receipt, including replies, with exact recipient and model-input provenance.",
    "admin",
    ["session", "delegation", "diagnostics"],
  ),
  writeCapability(
    "coordination.production.rounds.create",
    "POST",
    "/v1/coordination/rounds",
    "Start a durable operator correlated round on the production deployment.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "coordination.production.rounds.read",
    "GET",
    "/v1/coordination/rounds/{round_id}",
    "Read a production operator correlated round.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "coordination.production.deliveries.read",
    "GET",
    "/v1/coordination/deliveries/{delivery_id}",
    "Read a production operator delivery receipt.",
    "admin",
    ["session", "delegation"],
  ),
  readCapability(
    "review.submissions.diagnostics.list",
    "GET",
    "/v1/admin/diagnostics/review-submissions",
    "List Rust-owned durable review submission workflow state and adapter errors.",
    "admin",
    ["service", "diagnostics"],
  ),
  readCapability(
    "review.submissions.diagnostics.scope",
    "GET",
    "/v1/admin/diagnostics/review-submission-scope",
    "Read managed review project scope and distinguish Crew-managed records from direct Den reviews.",
    "admin",
    ["service", "diagnostics", "governance"],
  ),
  writeCapability(
    "review.submissions.external.create",
    "POST",
    "/v1/admin/review-submissions",
    "Submit an exact-SHA review from an unmanaged external agent; the service owns gates and routes only to @reviewer.",
    "admin",
    ["service", "governance"],
  ),
  readCapability(
    "review.submissions.external.read",
    "GET",
    "/v1/admin/review-submissions/{submission_id}",
    "Read the bounded receipt for an external exact-SHA review submission.",
    "admin",
    ["service", "governance"],
  ),
  writeCapability(
    "review.submissions.external.recover",
    "POST",
    "/v1/admin/review-submissions/{submission_id}/recover",
    "Reconcile one exact durable reviewer dispatch and redispatch only when its Rust-owned inbox state is no longer active.",
    "admin",
    ["service", "governance", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.agents.list",
    "GET",
    "/v1/debug/coordination/agents",
    "List recipients on the explicitly configured debug deployment.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.routes.list",
    "GET",
    "/v1/debug/coordination/routes",
    "List debug switchboard routes and current exact-target resolution status.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.routes.create",
    "POST",
    "/v1/debug/coordination/routes",
    "Create a revisioned debug switchboard route.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.routes.read",
    "GET",
    "/v1/debug/coordination/routes/{route_key}",
    "Read one debug switchboard route and current resolution status.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.routes.update",
    "PATCH",
    "/v1/debug/coordination/routes/{route_key}",
    "Replace one debug switchboard route with optimistic revision control.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.routes.delete",
    "DELETE",
    "/v1/debug/coordination/routes/{route_key}",
    "Delete one debug switchboard route with optimistic revision control.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.routes.resolve",
    "POST",
    "/v1/debug/coordination/routes/resolve",
    "Resolve an exact debug address without delivering a message.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.routes.test",
    "POST",
    "/v1/debug/coordination/routes/{route_key}/test",
    "Send a bounded test delivery through one debug switchboard route.",
    "admin",
    ["service", "session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.messages.create",
    "POST",
    "/v1/debug/coordination/messages",
    "Send a TTL-bound operator message on the explicitly configured debug deployment.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.messages.list",
    "GET",
    "/v1/debug/coordination/messages",
    "Inspect every durable debug delivery receipt, including replies, with exact recipient and model-input provenance.",
    "admin",
    ["session", "diagnostics"],
  ),
  writeCapability(
    "coordination.debug.rounds.create",
    "POST",
    "/v1/debug/coordination/rounds",
    "Start a durable operator correlated round on the explicitly configured debug deployment.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.rounds.read",
    "GET",
    "/v1/debug/coordination/rounds/{round_id}",
    "Read a debug operator correlated round.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "coordination.debug.deliveries.read",
    "GET",
    "/v1/debug/coordination/deliveries/{delivery_id}",
    "Read a debug operator delivery receipt.",
    "admin",
    ["session", "diagnostics"],
  ),
  readCapability(
    "admin.storage.schema",
    "GET",
    "/v1/admin/storage/schema",
    "Read backend-neutral module schema registry diagnostics.",
    "admin",
    ["storage", "diagnostics"],
  ),
  readCapability(
    "chat.sessions.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.sessions,
    "List Rusty View chat sessions.",
    "chat",
    ["chat", "session"],
  ),
  writeCapability(
    "chat.sessions.create",
    "POST",
    RUSTY_VIEW_CHAT_PATHS.sessions,
    "Create or recover a fresh Rust-owned Crew brain session from an active profile.",
    "chat",
    ["chat", "session", "profile"],
  ),
  readCapability(
    "chat.sessions.open",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.session,
    "Open a chat session.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.events",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.events,
    "List chat events for a session.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.stream",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.stream,
    "Stream chat events for a session.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.tool_calls.debug.get",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.toolCallDebug,
    "Read bounded redacted raw tool-call debug detail for a session.",
    "chat",
    ["chat", "session", "diagnostics", "tool"],
  ),
  readCapability(
    "chat.sessions.provider_requests.debug.get",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.providerRequestDebug,
    "Read bounded redacted provider request debug detail for a session.",
    "chat",
    ["chat", "session", "diagnostics"],
  ),
  {
    id: "chat.sessions.messages.create",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.messages,
    description: "Send a message to a chat session.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session"],
    public: true,
  },
  readCapability(
    "chat.sessions.slots.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.slots,
    "List primary message slots for a chat session.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  {
    id: "chat.sessions.slots.create",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.slots,
    description: "Create a primary message slot and primary variant.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  readCapability(
    "chat.sessions.slots.variants.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.slotVariants,
    "Lazy-load non-deleted variants for one message slot.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  {
    id: "chat.sessions.slots.variants.create",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.slotVariants,
    description: "Create an alternate variant for one message slot.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.slots.variants.delete",
    method: "DELETE",
    path_template: RUSTY_VIEW_CHAT_PATHS.slotVariant,
    description: "Delete an alternate message variant.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.slots.variants.reorder",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.reorderSlotVariants,
    description: "Reorder alternate variants for one message slot.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.slots.active_variant.select",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.activeSlotVariant,
    description:
      "Select the active variant for one message slot with conflict detection.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  readCapability(
    "chat.sessions.tree.open",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.tree,
    "Read the conversation branch and snapshot projection for a session.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  readCapability(
    "chat.sessions.jump.resolve",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.jump,
    "Resolve a message, branch, snapshot, or cursor jump target.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  readCapability(
    "chat.sessions.search",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.sessionSearch,
    "Search persisted transcript messages for one chat session.",
    "chat",
    ["chat", "session", "conversation", "search"],
  ),
  readCapability(
    "chat.search",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.search,
    "Search persisted transcript messages across chat sessions.",
    "chat",
    ["chat", "conversation", "search"],
  ),
  {
    id: "chat.sessions.branches.upsert",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.branches,
    description: "Create or update a conversation branch.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.branches.active.select",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.activeBranch,
    description:
      "Select the active conversation branch with conflict detection.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.branches.head.update",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.branchHead,
    description: "Update a branch head message with conflict detection.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  {
    id: "chat.sessions.snapshots.upsert",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.snapshots,
    description: "Create or update a conversation snapshot.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "conversation"],
    public: true,
  },
  readCapability(
    "chat.sessions.attachments.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.attachments,
    "List generic attachments for a chat session.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  {
    id: "chat.sessions.attachments.create",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.attachments,
    description:
      "Register uploaded attachment metadata and optional message, block, or scope links.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  {
    id: "chat.sessions.attachments.upload",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.attachmentUpload,
    description: "Upload Crew-owned raw image bytes for a chat attachment.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  {
    id: "chat.sessions.attachments.remove",
    method: "DELETE",
    path_template: RUSTY_VIEW_CHAT_PATHS.attachment,
    description: "Mark a chat attachment removed.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  readCapability(
    "chat.sessions.attachments.content",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.attachmentContent,
    "Read authenticated Crew-owned attachment bytes.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  readCapability(
    "chat.sessions.data_bank.scopes.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.dataBankScopes,
    "List reusable file scopes for a chat session.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  {
    id: "chat.sessions.data_bank.scopes.create",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.dataBankScopes,
    description: "Create or update a reusable file scope.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  {
    id: "chat.sessions.data_bank.scopes.remove",
    method: "DELETE",
    path_template: RUSTY_VIEW_CHAT_PATHS.dataBankScope,
    description: "Mark a reusable file scope removed.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  readCapability(
    "chat.sessions.data_bank.scopes.attachments.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.dataBankScopeAttachments,
    "List attachments linked to one reusable file scope.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  readCapability(
    "chat.commands.list",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.commands,
    "List browser-safe chat slash commands.",
    "chat",
    ["chat"],
  ),
  readCapability(
    "chat.commands.autocomplete",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.commandAutocomplete,
    "Resolve backend-provided autocomplete values for a chat slash command argument.",
    "chat",
    ["chat"],
  ),
  readCapability(
    "chat.sessions.context",
    "GET",
    RUSTY_VIEW_CHAT_PATHS.context,
    "Read browser-safe model/provider/brain and approximate context usage diagnostics for a chat session.",
    "chat",
    ["chat", "session", "diagnostics"],
  ),
  {
    id: "chat.sessions.context.compact",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.contextCompact,
    description:
      "Trigger a Rust-owned manual context compaction at a safe provider boundary without creating a transcript message or invoking the provider; idempotent via intent_key, persists artifact, returns terminal status.",
    auth: "chat",
    mutation: "control",
    stability: "experimental",
    tags: ["chat", "session", "diagnostics"],
    public: true,
  },
  {
    id: "chat.commands.execute",
    method: "POST",
    path_template: RUSTY_VIEW_CHAT_PATHS.sessionCommands,
    description: "Execute a chat slash command.",
    auth: "chat",
    mutation: "control",
    stability: "stable",
    tags: ["chat", "session"],
    public: true,
  },
  readCapability(
    "admin.capabilities",
    "GET",
    "/v1/admin/capabilities",
    "List public admin, chat, and control capabilities.",
    "admin",
    ["diagnostics", "service"],
  ),
  readCapability(
    "admin.mcp.servers",
    "GET",
    "/v1/admin/mcp/servers",
    "List configured MCP servers, tool profiles, and runtime bindings.",
    "admin",
    ["mcp", "config", "profile"],
  ),
  readCapability(
    "admin.roleplay.mechanic_sessions.list",
    "GET",
    "/v1/admin/roleplay/mechanic-sessions",
    "List typed roleplay mechanic conversation associations and runtime state.",
    "admin",
    ["profile", "storage", "diagnostics"],
  ),
  {
    id: "admin.roleplay.mechanic_sessions.create",
    method: "POST",
    path_template: "/v1/admin/roleplay/mechanic-sessions",
    description:
      "Create an independent mechanic conversation with an optional roleplay session attachment.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["profile", "storage", "diagnostics"],
    public: true,
  },
  readCapability(
    "admin.roleplay.mechanic_sessions.read",
    "GET",
    "/v1/admin/roleplay/mechanic-sessions/{mechanic_session_id}",
    "Read one mechanic conversation association and runtime state.",
    "admin",
    ["profile", "storage", "diagnostics"],
  ),
  ...(["attach", "archive", "restore"] as const).map((action) => ({
    id: `admin.roleplay.mechanic_sessions.${action}`,
    method: "POST" as const,
    path_template: `/v1/admin/roleplay/mechanic-sessions/{mechanic_session_id}/${action}`,
    description: `${action} a typed roleplay mechanic conversation.`,
    auth: "admin" as const,
    mutation: "control" as const,
    stability: "experimental" as const,
    tags: ["profile", "storage", "diagnostics"] as ApiCapabilityScope[],
    public: true,
  })),
  readCapability(
    "admin.roleplay.mechanic_diagnostics.list",
    "GET",
    "/v1/admin/roleplay/mechanic-diagnostics",
    "List durable roleplay mechanic diagnostic records and outcomes.",
    "admin",
    ["profile", "storage", "diagnostics"],
  ),
  {
    id: "admin.roleplay.mechanic_diagnostics.create",
    method: "POST",
    path_template: "/v1/admin/roleplay/mechanic-diagnostics",
    description:
      "Record a pending diagnostic from an attached mechanic conversation.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["profile", "storage", "diagnostics"],
    public: true,
  },
  readCapability(
    "admin.roleplay.mechanic_diagnostics.read",
    "GET",
    "/v1/admin/roleplay/mechanic-diagnostics/{diagnostic_id}",
    "Read one durable roleplay mechanic diagnostic.",
    "admin",
    ["profile", "storage", "diagnostics"],
  ),
  {
    id: "admin.roleplay.mechanic_diagnostics.outcome",
    method: "POST",
    path_template:
      "/v1/admin/roleplay/mechanic-diagnostics/{diagnostic_id}/outcome",
    description:
      "Record an evaluated diagnostic outcome with optimistic revision protection.",
    auth: "admin",
    mutation: "control",
    stability: "experimental",
    tags: ["profile", "storage", "diagnostics"],
    public: true,
  },
  readCapability(
    "admin.roleplay.mechanic_proposals.list",
    "GET",
    "/v1/admin/roleplay/mechanic-proposals",
    "List durable roleplay mechanic proposals and lifecycle state.",
    "admin",
    ["profile", "governance", "storage"],
  ),
  {
    id: "admin.roleplay.mechanic_proposals.create",
    method: "POST",
    path_template: "/v1/admin/roleplay/mechanic-proposals",
    description:
      "Create a durable roleplay mechanic proposal without mutating its target.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["profile", "governance", "storage"],
    public: true,
  },
  readCapability(
    "admin.roleplay.mechanic_proposals.read",
    "GET",
    "/v1/admin/roleplay/mechanic-proposals/{proposal_id}",
    "Read one durable roleplay mechanic proposal.",
    "admin",
    ["profile", "governance", "storage"],
  ),
  readCapability(
    "admin.roleplay.mechanic_proposals.history",
    "GET",
    "/v1/admin/roleplay/mechanic-proposals/{proposal_id}/history",
    "Read the audit history for one roleplay mechanic proposal.",
    "admin",
    ["profile", "governance", "diagnostics"],
  ),
  ...(["approve", "reject", "apply"] as const).map((action) => ({
    id: `admin.roleplay.mechanic_proposals.${action}`,
    method: "POST" as const,
    path_template: `/v1/admin/roleplay/mechanic-proposals/{proposal_id}/${action}`,
    description: `${action} a durable roleplay mechanic proposal through Rust-owned lifecycle authority.`,
    auth: "admin" as const,
    mutation: "control" as const,
    stability: "experimental" as const,
    tags: ["profile", "governance", "storage"] as ApiCapabilityScope[],
    public: true,
  })),
  {
    id: "admin.mcp.servers.create",
    method: "POST",
    path_template: "/v1/admin/mcp/servers",
    description:
      "Create or update a runtime-managed MCP server registry entry.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["mcp", "config", "profile"],
    public: true,
  },
  {
    id: "admin.mcp.servers.update",
    method: "PATCH",
    path_template: "/v1/admin/mcp/servers/{server_id}",
    description: "Update a runtime-managed MCP server registry entry.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["mcp", "config", "profile"],
    public: true,
  },
  {
    id: "admin.mcp.servers.delete",
    method: "DELETE",
    path_template: "/v1/admin/mcp/servers/{server_id}",
    description: "Delete a runtime-managed MCP server registry entry.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["mcp", "config", "profile"],
    public: true,
  },
  readCapability(
    "admin.tools.catalog",
    "GET",
    "/v1/admin/tools/catalog",
    "List built-in non-MCP tool policy sets and tool metadata.",
    "admin",
    ["tool", "profile", "config"],
  ),
  readCapability(
    "admin.brain_catalog.read",
    "GET",
    "/v1/admin/brains/catalog",
    "List canonical Rust-owned brain modules, strategies, protocols, and host capability requirements.",
    "admin",
    ["service", "profile", "config"],
  ),
  readCapability(
    "admin.context_strategies.catalog",
    "GET",
    "/v1/admin/context-strategies",
    "List model context strategy ids, policy defaults, and UI validation metadata.",
    "admin",
    ["service", "config", "profile"],
  ),
  readCapability(
    "admin.local_tool_profiles.list",
    "GET",
    "/v1/admin/local-tool-profiles",
    "List DB-backed local built-in tool profiles.",
    "admin",
    ["tool", "profile", "config"],
  ),
  {
    id: "admin.local_tool_profiles.create",
    method: "POST",
    path_template: "/v1/admin/local-tool-profiles",
    description: "Create a DB-backed local built-in tool profile.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["tool", "profile", "config"],
    public: true,
  },
  readCapability(
    "admin.local_tool_profiles.read",
    "GET",
    "/v1/admin/local-tool-profiles/{profile_id}",
    "Read one DB-backed local built-in tool profile.",
    "admin",
    ["tool", "profile", "config"],
  ),
  {
    id: "admin.local_tool_profiles.update",
    method: "PATCH",
    path_template: "/v1/admin/local-tool-profiles/{profile_id}",
    description: "Update one DB-backed local built-in tool profile.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["tool", "profile", "config"],
    public: true,
  },
  {
    id: "admin.local_tool_profiles.delete",
    method: "DELETE",
    path_template: "/v1/admin/local-tool-profiles/{profile_id}",
    description: "Delete one custom DB-backed local built-in tool profile.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["tool", "profile", "config"],
    public: true,
  },
  readCapability(
    "admin.image_generation.presets",
    "GET",
    "/v1/admin/image-generation/presets",
    "List approved image generation presets without provider secrets or workflow graphs.",
    "admin",
    ["tool", "media", "config"],
  ),
  {
    id: "admin.image_generation.generate",
    method: "POST",
    path_template: "/v1/admin/image-generation/generate",
    description:
      "Generate an image for a session through an approved server-side preset.",
    auth: "admin",
    mutation: "write",
    stability: "experimental",
    tags: ["tool", "media"],
    public: true,
  },
  readCapability(
    "admin.healthz",
    "GET",
    "/v1/admin/healthz",
    "Read service liveness.",
    "none",
    ["diagnostics"],
  ),
  readCapability(
    "admin.readyz",
    "GET",
    "/v1/admin/readyz",
    "Read service readiness.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics",
    "GET",
    "/v1/admin/diagnostics",
    "Read full diagnostics projection.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.overview",
    "GET",
    "/v1/admin/diagnostics/overview",
    "Read diagnostics overview.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.sessions",
    "GET",
    "/v1/admin/diagnostics/sessions",
    "List session diagnostics.",
    "admin",
    ["diagnostics", "session"],
  ),
  readCapability(
    "admin.diagnostics.agents",
    "GET",
    "/v1/admin/diagnostics/agents",
    "List agent diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.delegations",
    "GET",
    "/v1/admin/diagnostics/delegations",
    "List delegation diagnostics.",
    "admin",
    ["diagnostics", "delegation"],
  ),
  readCapability(
    "admin.diagnostics.queues",
    "GET",
    "/v1/admin/diagnostics/queues",
    "Read queue diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.tools",
    "GET",
    "/v1/admin/diagnostics/tools",
    "List tool diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.mcp",
    "GET",
    "/v1/admin/diagnostics/mcp",
    "List MCP diagnostics.",
    "admin",
    ["diagnostics", "mcp"],
  ),
  readCapability(
    "admin.diagnostics.channels",
    "GET",
    "/v1/admin/diagnostics/channels",
    "List channel binding diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.persistence",
    "GET",
    "/v1/admin/diagnostics/persistence",
    "Read persistence diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.storage",
    "GET",
    "/v1/admin/diagnostics/storage",
    "Read backend storage diagnostics and capability projection.",
    "admin",
    ["diagnostics", "storage"],
  ),
  readCapability(
    "admin.diagnostics.memory_surfaces",
    "GET",
    "/v1/admin/diagnostics/memory-surfaces",
    "Read the operator catalog of memory-like surfaces, ownership, tools, provenance, and availability.",
    "admin",
    ["diagnostics", "memory"],
  ),
  readCapability(
    "admin.diagnostics.built_in_skills",
    "GET",
    "/v1/admin/diagnostics/built-in-skills",
    "Read immutable built-in skill registration and prompt-pointer health.",
    "admin",
    ["diagnostics", "tool"],
  ),
  readCapability(
    "admin.diagnostics.profiles",
    "GET",
    "/v1/admin/diagnostics/profiles",
    "Read DB-backed profile registry and asset drift diagnostics.",
    "admin",
    ["diagnostics", "profile"],
  ),
  readCapability(
    "admin.profiles.registry.list",
    "GET",
    "/v1/admin/profiles/registry",
    "List DB-backed profile registry records.",
    "admin",
    ["diagnostics", "profile"],
  ),
  readCapability(
    "admin.profiles.registry.read",
    "GET",
    "/v1/admin/profiles/registry/{profile_id}",
    "Read one DB-backed profile registry record.",
    "admin",
    ["diagnostics", "profile"],
  ),
  readCapability(
    "admin.profiles.registry.export_plan",
    "GET",
    "/v1/admin/profiles/registry/{profile_id}/export-plan",
    "Plan a profile bundle export from registry state and file-backed assets without embedding raw prompt file contents.",
    "admin",
    ["diagnostics", "profile"],
  ),
  mutationCapability(
    "admin.profiles.registry.update_plan",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.updatePlan,
    "Plan DB-backed profile registry field changes without editing profile files or service.json.",
    "admin",
    ["profile", "config"],
  ),
  mutationCapability(
    "admin.profiles.registry.update_apply",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.updateApply,
    "Apply DB-backed profile registry field changes with revision checking.",
    "admin",
    ["profile", "config"],
  ),
  mutationCapability(
    "admin.profiles.registry.lifecycle_plan",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.lifecyclePlan,
    "Plan a DB-backed profile lifecycle transition.",
    "admin",
    ["profile", "config"],
  ),
  mutationCapability(
    "admin.profiles.registry.lifecycle_apply",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.lifecycleApply,
    "Apply a DB-backed profile lifecycle transition and run safe runtime effects.",
    "admin",
    ["profile", "config"],
  ),
  mutationCapability(
    "admin.profiles.registry.prompt_plan",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.promptPlan,
    "Plan DB-backed profile soul and memory prompt text changes.",
    "admin",
    ["profile", "config", "prompt"],
  ),
  mutationCapability(
    "admin.profiles.registry.prompt_apply",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.promptApply,
    "Apply DB-backed profile soul and memory prompt text changes with revision checking.",
    "admin",
    ["profile", "config", "prompt"],
  ),
  mutationCapability(
    "admin.profiles.registry.runtime_config_plan",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.runtimeConfigPlan,
    "Plan DB-backed profile provider, built-in tool policy, and MCP binding changes without applying them.",
    "admin",
    ["profile", "config", "tool", "mcp"],
  ),
  mutationCapability(
    "admin.profiles.registry.runtime_config_apply",
    "POST",
    PROFILE_REGISTRY_ADMIN_PATHS.runtimeConfigApply,
    "Apply DB-backed profile provider, built-in tool policy, and MCP binding changes with revision checking and runtime reload.",
    "admin",
    ["profile", "config", "tool", "mcp"],
  ),
  readCapability(
    "admin.telegram_diplomat.read",
    "GET",
    "/v1/admin/telegram-diplomat",
    "Read Telegram diplomat configuration, redacted credential state, bot identity, discovered chat and topic candidates, bindings, and connector diagnostics.",
    "admin",
    ["service", "diagnostics"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.credential.update",
    "POST",
    "/v1/admin/telegram-diplomat/credential",
    "Create or rotate the Telegram bot token in the service credential store and reload the connector.",
    "admin",
    ["service", "config"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.reload",
    "POST",
    "/v1/admin/telegram-diplomat/reload",
    "Reload the Telegram connector from the current service credential and durable bindings.",
    "admin",
    ["service", "config"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.create",
    "POST",
    "/v1/admin/telegram-diplomat/bindings",
    "Bind an identified install bot and exact Telegram chat or topic to one full Crew session.",
    "admin",
    ["service", "session"],
  ),
  readCapability(
    "admin.telegram_diplomat.bindings.read",
    "GET",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}",
    "Read one exact session-scoped Telegram diplomat binding.",
    "admin",
    ["service", "session"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.move",
    "POST",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}/move",
    "Move only the Telegram surface binding to another exact full session.",
    "admin",
    ["service", "session"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.relabel",
    "POST",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}/relabel",
    "Relabel the installation represented by a diplomat binding without changing its session, profile, or workspace.",
    "admin",
    ["service", "session"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.pause",
    "POST",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}/pause",
    "Pause one Telegram diplomat binding with revision protection.",
    "admin",
    ["service", "session"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.resume",
    "POST",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}/resume",
    "Resume one Telegram diplomat binding with revision protection.",
    "admin",
    ["service", "session"],
  ),
  mutationCapability(
    "admin.telegram_diplomat.bindings.remove",
    "POST",
    "/v1/admin/telegram-diplomat/bindings/{binding_id}/remove",
    "Remove one Telegram diplomat binding without archiving its session or changing a profile.",
    "admin",
    ["service", "session"],
  ),
  readCapability(
    "admin.storage.query_catalog",
    "GET",
    "/v1/admin/storage/query-catalog",
    "List curated read-only storage queries.",
    "admin",
    ["storage"],
  ),
  {
    id: "admin.storage.query.execute",
    method: "POST",
    path_template: "/v1/admin/storage/query/{query_id}",
    description:
      "Execute one curated read-only storage query by id. Raw SQL is not supported.",
    auth: "admin",
    mutation: "read",
    stability: "stable",
    tags: ["storage", "diagnostics"],
    public: true,
  },
  readCapability(
    "admin.memory.spaces.list",
    "GET",
    "/v1/admin/memory/spaces",
    "List Rusty Crew runtime-owned memory-space descriptors.",
    "admin",
    ["memory", "diagnostics"],
  ),
  readCapability(
    "admin.memory.spaces.read",
    "GET",
    "/v1/admin/memory/spaces/{space_id}",
    "Read one Rusty Crew memory-space descriptor.",
    "admin",
    ["memory", "diagnostics"],
  ),
  readCapability(
    "admin.memory.spaces.records.list",
    "GET",
    "/v1/admin/memory/spaces/{space_id}/records",
    "List bounded records for a supported Rusty Crew memory space.",
    "admin",
    ["memory"],
  ),
  readCapability(
    "admin.memory.spaces.records.read",
    "GET",
    "/v1/admin/memory/spaces/{space_id}/records/{key}",
    "Read one record for a supported Rusty Crew memory space.",
    "admin",
    ["memory"],
  ),
  readCapability(
    "admin.memory.proposals.list",
    "GET",
    "/v1/admin/memory/proposals",
    "List typed Rusty Crew memory proposals for review surfaces.",
    "admin",
    ["memory", "governance"],
  ),
  mutationCapability(
    "admin.memory.proposals.create",
    "POST",
    "/v1/admin/memory/proposals",
    "Create a typed Rusty Crew memory proposal without directly mutating memory records.",
    "admin",
    ["memory", "governance"],
  ),
  mutationCapability(
    "admin.memory.proposals.decide",
    "POST",
    "/v1/admin/memory/proposals/{proposal_id}/decisions",
    "Record a typed Rusty Crew memory governance decision.",
    "admin",
    ["memory", "governance"],
  ),
  readCapability(
    "admin.diagnostics.provider_state",
    "GET",
    "/v1/admin/diagnostics/provider-state",
    "Read provider wire-state diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.buffered_brain_runs",
    "GET",
    "/v1/admin/diagnostics/buffered-brain-runs",
    "Read metadata-only diagnostics for active Rust brain buffered runs.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.activities",
    "GET",
    "/v1/admin/diagnostics/activities",
    "Read the Rust-owned runtime activity census and reconciliation findings; use sessionProjection=durable to compare against persisted session state.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.observation",
    "GET",
    "/v1/admin/diagnostics/observation",
    "Read observation diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.background",
    "GET",
    "/v1/admin/diagnostics/background",
    "Read background service diagnostics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.config",
    "GET",
    "/v1/admin/diagnostics/config",
    "Read config validation diagnostics.",
    "admin",
    ["diagnostics", "config"],
  ),
  readCapability(
    "admin.curator.candidates.list",
    "GET",
    "/v1/admin/curator/candidates",
    "List typed curator candidates with bounded paging and lifecycle filters.",
    "admin",
    ["curator", "diagnostics"],
  ),
  readCapability(
    "admin.curator.mutations.list",
    "GET",
    "/v1/admin/curator/mutations",
    "List typed curator mutation and rollback history with bounded paging.",
    "admin",
    ["curator", "diagnostics"],
  ),
  readCapability(
    "admin.curator.audit_receipts.list",
    "GET",
    "/v1/admin/curator/audit-receipts",
    "List sequenced neutral curator audit receipts with bounded paging.",
    "admin",
    ["curator", "diagnostics"],
  ),
  readCapability(
    "admin.diagnostics.metrics",
    "GET",
    "/v1/admin/diagnostics/metrics",
    "Read service metrics.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.events.recent",
    "GET",
    "/v1/admin/events/recent",
    "List recent service events.",
    "admin",
    ["diagnostics"],
  ),
  readCapability(
    "admin.scheduler.jobs",
    "GET",
    "/v1/admin/scheduler/jobs",
    "List scheduler jobs.",
    "admin",
    ["scheduler"],
  ),
  readCapability(
    "admin.scheduler.runs",
    "GET",
    "/v1/admin/scheduler/runs",
    "List scheduler runs.",
    "admin",
    ["scheduler"],
  ),
  ...ADMIN_CONTROL_CAPABILITIES,
] as const satisfies readonly ApiCapabilityDescriptor[];

export function slashCommandNames(): SlashCommandName[] {
  return SLASH_COMMAND_REGISTRY.map((command) => command.name);
}

export function findSlashCommandDescriptor(
  nameOrAlias: string,
): SlashCommandDescriptor | undefined {
  const normalized = nameOrAlias.toLowerCase().replace(/^\//, "");
  return SLASH_COMMAND_REGISTRY.find(
    (command) =>
      command.name === normalized ||
      command.aliases.some((alias) => alias.replace(/^\//, "") === normalized),
  );
}

export function chatCommandRegistry(): ChatCommandRegistry {
  return {
    commands: SLASH_COMMAND_REGISTRY.map(chatCommandDescriptor),
  };
}

export function chatCommandAutocomplete(input: {
  commandName: string;
  argumentName: string;
  query?: string;
  limit?: number;
}): ChatCommandAutocompleteResult | undefined {
  const command = findSlashCommandDescriptor(input.commandName);
  if (!command) return undefined;
  const argument = [...command.positionalArgs, ...command.namedArgs].find(
    (candidate) => candidate.name === input.argumentName,
  );
  if (!argument) return undefined;
  const provider = argument.enum_provider;
  const staticItems = argument.enum_values ?? [];
  const query = input.query?.trim().toLowerCase();
  const limit = Math.max(1, Math.min(input.limit ?? 20, 100));
  const filtered = staticItems.filter(
    (item) =>
      query === undefined ||
      item.value.toLowerCase().includes(query) ||
      item.label?.toLowerCase().includes(query) ||
      item.description?.toLowerCase().includes(query),
  );
  return {
    command_name: command.name,
    argument_name: argument.name,
    provider,
    items: filtered.slice(0, limit).map((item) => ({ ...item })),
    has_more: filtered.length > limit,
  };
}

export function apiCapabilityRegistry(): ApiCapabilityRegistry {
  return {
    schema_version: 1,
    slash_commands: chatCommandRegistry().commands,
    capabilities: API_CAPABILITIES.map((capability) => ({ ...capability })),
  };
}

export function chatApiCapabilityPaths(): string[] {
  return [
    ...new Set(
      API_CAPABILITIES.filter((capability) =>
        (capability.tags as readonly ApiCapabilityScope[]).includes("chat"),
      ).map((capability) => capability.path_template),
    ),
  ];
}

function slashCommand<const Name extends string>(input: {
  name: Name;
  description: string;
  readOnly: boolean;
  positionalArgs?: readonly ChatCommandArgumentDescriptor[];
  namedArgs?: readonly ChatCommandArgumentDescriptor[];
  surfaces?: readonly ChatCommandSurface[];
  control?: SlashCommandDefinition["control"];
}): SlashCommandDefinition<Name> {
  const mutating = !input.readOnly;
  return {
    name: input.name,
    aliases: [`/${input.name}`],
    description: input.description,
    argsSchema: OPTIONAL_ARGS_SCHEMA,
    positionalArgs: input.positionalArgs ?? [],
    namedArgs: input.namedArgs ?? [],
    surfaces: input.surfaces ?? ["chat-input", "global"],
    source: input.control ? "backend-control" : "backend",
    readOnly: input.readOnly,
    mutating,
    scope: "session",
    allowedSessionKinds: mutating ? ["full"] : ["full", "worker", "delegated"],
    requiresControlAuth: mutating,
    control: input.control,
  };
}

function chatCommandDescriptor(
  command: SlashCommandDescriptor,
): ChatCommandDescriptor {
  return {
    name: command.name,
    aliases: [...command.aliases],
    description: command.description,
    args_schema: { ...command.argsSchema },
    positional_args: command.positionalArgs.map(cloneArgumentDescriptor),
    named_args: command.namedArgs.map(cloneArgumentDescriptor),
    surfaces: [...command.surfaces],
    source: command.source,
    read_only: command.readOnly,
    mutating: command.mutating,
    scope: command.scope,
    allowed_session_kinds: [...command.allowedSessionKinds],
    requires_control_auth: command.requiresControlAuth,
    backing_control_command: command.control?.commandName,
    ...(command.control?.rustPlanOperation === undefined
      ? {}
      : { rust_plan_operation: command.control.rustPlanOperation }),
  };
}

function cloneArgumentDescriptor(
  argument: ChatCommandArgumentDescriptor,
): ChatCommandArgumentDescriptor {
  const clone: ChatCommandArgumentDescriptor = { ...argument };
  if (argument.enum_values) {
    clone.enum_values = argument.enum_values.map((item) => ({ ...item }));
  }
  return clone;
}

function readCapability(
  id: string,
  method: "GET",
  pathTemplate: string,
  description: string,
  auth: ApiCapabilityAuth,
  tags: ApiCapabilityScope[],
): ApiCapabilityDescriptor {
  return {
    id,
    method,
    path_template: pathTemplate,
    description,
    auth,
    mutation: "read",
    stability: "stable",
    tags,
    public: true,
  };
}

function writeCapability(
  id: string,
  method: "DELETE" | "PATCH" | "POST",
  pathTemplate: string,
  description: string,
  auth: ApiCapabilityAuth,
  tags: ApiCapabilityScope[],
): ApiCapabilityDescriptor {
  return {
    id,
    method,
    path_template: pathTemplate,
    description,
    auth,
    mutation: "write",
    stability: "experimental",
    tags,
    public: true,
  };
}

function controlApiCapability(
  id: string,
  method: "POST",
  pathTemplate: string,
  description: string,
  tags: ApiCapabilityScope[],
): ApiCapabilityDescriptor {
  return {
    id,
    method,
    path_template: pathTemplate,
    description,
    auth: "admin",
    mutation: "control",
    stability: "experimental",
    tags,
    public: true,
  };
}

function mutationCapability(
  id: string,
  method: "POST",
  pathTemplate: string,
  description: string,
  auth: ApiCapabilityAuth,
  tags: ApiCapabilityScope[],
): ApiCapabilityDescriptor {
  return {
    id,
    method,
    path_template: pathTemplate,
    description,
    auth,
    mutation: "control",
    stability: "stable",
    tags,
    public: true,
  };
}

function controlCapability(
  id: string,
  method: "POST",
  pathTemplate: string,
  description: string,
  commandName: AdminControlCommandName,
  tags: ApiCapabilityScope[],
  options: { rustPlanOperation?: string } = {},
): ApiCapabilityDescriptor {
  return {
    id,
    method,
    path_template: pathTemplate,
    description,
    auth: "admin",
    mutation: "control",
    stability: "stable",
    tags,
    public: true,
    command_name: commandName,
    ...(options.rustPlanOperation === undefined
      ? {}
      : { rust_plan_operation: options.rustPlanOperation }),
  };
}
