import type { AdminControlCommandName } from "./admin-control-api.js";
import {
  findSlashCommandDescriptor,
  slashCommandNames,
  type SlashCommandName,
} from "./api-command-registry.js";
import {
  isNativeReasoningEffort,
  nativeReasoningEffortList,
} from "./reasoning-effort-policy.js";

export type { SlashCommandName } from "./api-command-registry.js";

export type SlashCommandStatus = "ok" | "denied" | "invalid";

export interface SlashCommandSession {
  sessionId: string;
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  reasoningEffortOverride?: string;
}

export interface SlashCommandActor {
  id: string;
  displayName?: string;
}

export interface SlashCommandInput {
  text: string;
  session: SlashCommandSession;
  actor: SlashCommandActor;
  source?: {
    adapterId?: string;
    bindingId?: string;
    channelId?: string | number;
    messageId?: string | number;
  };
  options?: SlashCommandRouterOptions;
}

export interface SlashCommandRouterOptions {
  primeProfiles?: readonly string[];
  allowedCommands?: readonly SlashCommandName[];
  allowNonPrimeReadCommands?: boolean;
  allowWorkerControls?: boolean;
}

export interface SlashCommandResponse {
  title: string;
  summary: string;
  fields?: Record<string, string | number | boolean>;
  items?: string[];
}

export interface SlashCommandControlRequest {
  commandName: AdminControlCommandName;
  target: Record<string, string>;
  reason: string;
  reasonCode: string;
  body: Record<string, unknown>;
}

export type SlashCommandRouteResult =
  | {
      kind: "pass_through";
      text: string;
    }
  | {
      kind: "intercepted";
      commandName: SlashCommandName | "unknown";
      status: SlashCommandStatus;
      response: SlashCommandResponse;
      controlRequest?: SlashCommandControlRequest;
    };

export function routeSlashCommand(
  input: SlashCommandInput,
): SlashCommandRouteResult {
  const parsed = parseSlashCommand(input.text);
  if (!parsed) return { kind: "pass_through", text: input.text };

  const commandName = normalizeCommandName(parsed.name);
  if (!commandName) {
    return intercepted(
      "unknown",
      "invalid",
      unknownCommandResponse(parsed.name),
    );
  }

  if (!commandAllowed(commandName, input.options)) {
    return intercepted(commandName, "denied", {
      title: "Command unavailable",
      summary: `/${commandName} is not available on this surface.`,
    });
  }

  const authorization = authorizeSlashCommand(
    commandName,
    parsed,
    input.session,
    input.options,
  );
  if (!authorization.allowed) {
    return intercepted(commandName, "denied", {
      title: "Command denied",
      summary: authorization.reason,
      fields: {
        sessionId: input.session.sessionId,
        profileId: input.session.profileId,
        kind: input.session.kind,
      },
    });
  }

  return SLASH_COMMAND_HANDLERS[commandName](input, parsed);
}

interface ParsedSlashCommand {
  name: string;
  args: string;
}

type SlashCommandHandler = (
  input: SlashCommandInput,
  parsed: ParsedSlashCommand,
) => SlashCommandRouteResult;

const SLASH_COMMAND_HANDLERS = {
  help: (input) => intercepted("help", "ok", helpResponse(input.options)),
  status: (input) =>
    intercepted("status", "ok", {
      title: "Status",
      summary: "Diagnostics status requested.",
      fields: sessionFields(input.session),
    }),
  session: (input) =>
    intercepted("session", "ok", {
      title: "Session",
      summary: "Session summary requested.",
      fields: sessionFields(input.session),
    }),
  model: (input) =>
    intercepted("model", "ok", {
      title: "Model",
      summary: "Model and brain diagnostics requested.",
      fields: sessionFields(input.session),
    }),
  effort: (input, parsed) => {
    const raw = parsed.args.trim();
    if (raw.length === 0) {
      return intercepted("effort", "ok", {
        title: "Reasoning Effort",
        summary: "Reasoning effort diagnostics requested.",
        fields: sessionFields(input.session),
      });
    }
    const effort = raw.toLowerCase();
    if (effort !== "default" && !isNativeReasoningEffort(effort)) {
      return intercepted("effort", "invalid", {
        title: "Invalid reasoning effort",
        summary: `Reasoning effort must be default or one of: ${nativeReasoningEffortList()}.`,
        fields: { reasonCode: "invalid_reasoning_effort" },
      });
    }
    return intercepted(
      "effort",
      "ok",
      {
        title: "Reasoning Effort",
        summary:
          effort === "default"
            ? "Clear the current session reasoning-effort override."
            : `Set current session reasoning effort to ${effort}.`,
        fields: sessionFields(input.session),
      },
      {
        commandName: "set_session_effort",
        target: { sessionId: input.session.sessionId },
        reason: `slash command /effort ${effort}`,
        reasonCode: "slash_set_session_effort",
        body: {
          reasoningEffort: effort === "default" ? null : effort,
          ...controlBody(input, parsed.args),
        },
      },
    );
  },
  new: (input, parsed) =>
    intercepted(
      "new",
      "ok",
      {
        title: "New Session",
        summary: "Archive current session and create a fresh session.",
        fields: sessionFields(input.session),
      },
      {
        commandName: "new_session",
        target: { sessionId: input.session.sessionId },
        reason: parsed.args || "slash command /new",
        reasonCode: "slash_new_session",
        body: controlBody(input, parsed.args),
      },
    ),
  "reload-mcp": (input, parsed) =>
    intercepted(
      "reload-mcp",
      "ok",
      {
        title: "Reload MCP",
        summary: "Reload MCP for the current session.",
        fields: sessionFields(input.session),
      },
      {
        commandName: "reload_mcp",
        target: { sessionId: input.session.sessionId },
        reason: parsed.args || "slash command /reload-mcp",
        reasonCode: "slash_reload_mcp",
        body: controlBody(input, parsed.args),
      },
    ),
} satisfies Record<SlashCommandName, SlashCommandHandler>;

export function slashCommandHandlerNames(): SlashCommandName[] {
  return Object.keys(SLASH_COMMAND_HANDLERS) as SlashCommandName[];
}

function parseSlashCommand(text: string): ParsedSlashCommand | undefined {
  const trimmed = text.trim();
  if (!trimmed.startsWith("/")) return undefined;
  const withoutSlash = trimmed.slice(1);
  const [name = "", ...rest] = withoutSlash.split(/\s+/);
  return {
    name,
    args: rest.join(" ").trim(),
  };
}

function normalizeCommandName(name: string): SlashCommandName | undefined {
  return findSlashCommandDescriptor(name)?.name;
}

function commandAllowed(
  commandName: SlashCommandName,
  options: SlashCommandRouterOptions | undefined,
): boolean {
  return (
    options?.allowedCommands === undefined ||
    options.allowedCommands.includes(commandName)
  );
}

function authorizeSlashCommand(
  commandName: SlashCommandName,
  parsed: ParsedSlashCommand,
  session: SlashCommandSession,
  options: SlashCommandRouterOptions | undefined,
): { allowed: true } | { allowed: false; reason: string } {
  const isControl =
    commandName === "effort" && parsed.args.length === 0
      ? false
      : (findSlashCommandDescriptor(commandName)?.mutating ?? false);
  const isPrime = (options?.primeProfiles ?? ["prime"]).includes(
    session.profileId,
  );
  const isFullPrime = session.kind === "full" && isPrime;

  if (!isControl && (isFullPrime || options?.allowNonPrimeReadCommands)) {
    return { allowed: true };
  }
  if (isControl && (isFullPrime || options?.allowWorkerControls)) {
    return { allowed: true };
  }
  return {
    allowed: false,
    reason: isControl
      ? "Control commands require an authorized full/prime session."
      : "Read-only commands require an authorized session on this surface.",
  };
}

function intercepted(
  commandName: SlashCommandName | "unknown",
  status: SlashCommandStatus,
  response: SlashCommandResponse,
  controlRequest?: SlashCommandControlRequest,
): SlashCommandRouteResult {
  return {
    kind: "intercepted",
    commandName,
    status,
    response,
    controlRequest,
  };
}

function helpResponse(
  options: SlashCommandRouterOptions | undefined,
): SlashCommandResponse {
  const commands = slashCommandNames().filter((command) =>
    commandAllowed(command, options),
  );
  return {
    title: "Commands",
    summary: "Available slash commands.",
    items: commands.map((command) => `/${command}`),
  };
}

function unknownCommandResponse(name: string): SlashCommandResponse {
  return {
    title: "Unknown Command",
    summary: `Unknown slash command /${name}.`,
    items: slashCommandNames().map((command) => `/${command}`),
  };
}

function sessionFields(
  session: SlashCommandSession,
): Record<string, string | number | boolean> {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
  };
}

function controlBody(
  input: SlashCommandInput,
  reason: string,
): Record<string, unknown> {
  return {
    reason,
    actorId: input.actor.id,
    source: input.source,
  };
}
