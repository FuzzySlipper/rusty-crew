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
export type ApiCapabilityScope =
  | "chat"
  | "diagnostics"
  | "profile"
  | "session"
  | "delegation"
  | "mcp"
  | "config"
  | "maintenance"
  | "scheduler"
  | "curator"
  | "service";

export interface SlashCommandDescriptor {
  name: SlashCommandName;
  aliases: readonly string[];
  description: string;
  argsSchema: Record<string, unknown>;
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
  read_only: boolean;
  mutating: boolean;
  scope: "session" | "profile" | "service";
  allowed_session_kinds: Array<"full" | "worker" | "delegated">;
  requires_control_auth: boolean;
  backing_control_command?: AdminControlCommandName;
}

export interface ApiCapabilityDescriptor {
  id: string;
  method: "GET" | "POST";
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
    "chat.commands.list",
    "GET",
    "/v1/chat/commands",
    "List browser-safe chat slash commands.",
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

export function apiCapabilityRegistry(): ApiCapabilityRegistry {
  return {
    schema_version: 1,
    slash_commands: chatCommandRegistry().commands,
    capabilities: API_CAPABILITIES.map((capability) => ({ ...capability })),
  };
}

export function chatApiCapabilityPaths(): string[] {
  return API_CAPABILITIES.filter((capability) =>
    capability.tags.includes("chat"),
  ).map((capability) => capability.path_template);
}

function slashCommand(input: {
  name: SlashCommandName;
  description: string;
  readOnly: boolean;
  control?: SlashCommandDescriptor["control"];
}): SlashCommandDescriptor {
  const mutating = !input.readOnly;
  return {
    name: input.name,
    aliases: [`/${input.name}`],
    description: input.description,
    argsSchema: OPTIONAL_ARGS_SCHEMA,
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
    read_only: command.readOnly,
    mutating: command.mutating,
    scope: command.scope,
    allowed_session_kinds: [...command.allowedSessionKinds],
    requires_control_auth: command.requiresControlAuth,
    backing_control_command: command.control?.commandName,
  };
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
