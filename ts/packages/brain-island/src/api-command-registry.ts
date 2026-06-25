import type { AdminControlCommandName } from "./admin-control-api.js";

export type SlashCommandName =
  | "help"
  | "status"
  | "session"
  | "new"
  | "reload-mcp";

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
  | "maintenance"
  | "scheduler"
  | "search"
  | "curator"
  | "service";

export interface SlashCommandDescriptor {
  name: SlashCommandName;
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
  };
}

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
  method: "DELETE" | "GET" | "POST";
  path_template: string;
  description: string;
  auth: ApiCapabilityAuth;
  mutation: ApiCapabilityMutation;
  stability: ApiCapabilityStability;
  tags: ApiCapabilityScope[];
  public: boolean;
  command_name?: AdminControlCommandName;
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
    name: "new",
    description:
      "Archive the current session and create a fresh replacement session.",
    readOnly: false,
    positionalArgs: [OPTIONAL_REASON_ARGUMENT],
    control: {
      commandName: "new_session",
      pathTemplate: "/v1/admin/control/sessions/{session_id}/new",
      reasonCode: "slash_new_session",
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
    },
  }),
] as const satisfies readonly SlashCommandDescriptor[];

export const ADMIN_CONTROL_CAPABILITIES = [
  controlCapability(
    "admin.control.profiles.create",
    "POST",
    "/v1/admin/control/profiles",
    "Create a profile and related service plumbing.",
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
    "admin.control.sessions.create",
    "POST",
    "/v1/admin/control/sessions",
    "Create a runtime session.",
    "create_session",
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
    "chat.sessions.list",
    "GET",
    "/v1/chat/sessions",
    "List Rusty View chat sessions.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.open",
    "GET",
    "/v1/chat/sessions/{session_id}",
    "Open a chat session.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.events",
    "GET",
    "/v1/chat/sessions/{session_id}/events",
    "List chat events for a session.",
    "chat",
    ["chat", "session"],
  ),
  readCapability(
    "chat.sessions.stream",
    "GET",
    "/v1/chat/sessions/{session_id}/stream",
    "Stream chat events for a session.",
    "chat",
    ["chat", "session"],
  ),
  {
    id: "chat.sessions.messages.create",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/messages",
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
    "/v1/chat/sessions/{session_id}/slots",
    "List primary message slots for a chat session.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  {
    id: "chat.sessions.slots.create",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/slots",
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
    "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants",
    "Lazy-load non-deleted variants for one message slot.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  {
    id: "chat.sessions.slots.variants.create",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants",
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
    path_template:
      "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/{variant_id}",
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
    path_template:
      "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/reorder",
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
    path_template:
      "/v1/chat/sessions/{session_id}/slots/{slot_id}/active-variant",
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
    "/v1/chat/sessions/{session_id}/tree",
    "Read the conversation branch and snapshot projection for a session.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  readCapability(
    "chat.sessions.jump.resolve",
    "GET",
    "/v1/chat/sessions/{session_id}/jump",
    "Resolve a message, branch, snapshot, or cursor jump target.",
    "chat",
    ["chat", "session", "conversation"],
  ),
  readCapability(
    "chat.sessions.search",
    "GET",
    "/v1/chat/sessions/{session_id}/search",
    "Search persisted transcript messages for one chat session.",
    "chat",
    ["chat", "session", "conversation", "search"],
  ),
  readCapability(
    "chat.search",
    "GET",
    "/v1/chat/search",
    "Search persisted transcript messages across chat sessions.",
    "chat",
    ["chat", "conversation", "search"],
  ),
  {
    id: "chat.sessions.branches.upsert",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/branches",
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
    path_template: "/v1/chat/sessions/{session_id}/branches/active",
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
    path_template: "/v1/chat/sessions/{session_id}/branches/{branch_id}/head",
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
    path_template: "/v1/chat/sessions/{session_id}/snapshots",
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
    "/v1/chat/sessions/{session_id}/attachments",
    "List generic attachments for a chat session.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  {
    id: "chat.sessions.attachments.create",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/attachments",
    description:
      "Register uploaded attachment metadata and optional message, block, or scope links.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  {
    id: "chat.sessions.attachments.remove",
    method: "DELETE",
    path_template: "/v1/chat/sessions/{session_id}/attachments/{attachment_id}",
    description: "Mark a chat attachment removed.",
    auth: "chat",
    mutation: "write",
    stability: "stable",
    tags: ["chat", "session", "attachment"],
    public: true,
  },
  readCapability(
    "chat.sessions.data_bank.scopes.list",
    "GET",
    "/v1/chat/sessions/{session_id}/data-bank/scopes",
    "List reusable file scopes for a chat session.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  {
    id: "chat.sessions.data_bank.scopes.create",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/data-bank/scopes",
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
    path_template: "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}",
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
    "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}/attachments",
    "List attachments linked to one reusable file scope.",
    "chat",
    ["chat", "session", "attachment"],
  ),
  readCapability(
    "chat.commands.list",
    "GET",
    "/v1/chat/commands",
    "List browser-safe chat slash commands.",
    "chat",
    ["chat"],
  ),
  readCapability(
    "chat.commands.autocomplete",
    "GET",
    "/v1/chat/commands/{command_name}/autocomplete",
    "Resolve backend-provided autocomplete values for a chat slash command argument.",
    "chat",
    ["chat"],
  ),
  {
    id: "chat.commands.execute",
    method: "POST",
    path_template: "/v1/chat/sessions/{session_id}/commands",
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
    "admin.diagnostics.provider_state",
    "GET",
    "/v1/admin/diagnostics/provider-state",
    "Read provider wire-state diagnostics.",
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
        capability.tags.includes("chat"),
      ).map((capability) => capability.path_template),
    ),
  ];
}

function slashCommand(input: {
  name: SlashCommandName;
  description: string;
  readOnly: boolean;
  positionalArgs?: readonly ChatCommandArgumentDescriptor[];
  namedArgs?: readonly ChatCommandArgumentDescriptor[];
  surfaces?: readonly ChatCommandSurface[];
  control?: SlashCommandDescriptor["control"];
}): SlashCommandDescriptor {
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

function controlCapability(
  id: string,
  method: "POST",
  pathTemplate: string,
  description: string,
  commandName: AdminControlCommandName,
  tags: ApiCapabilityScope[],
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
  };
}
