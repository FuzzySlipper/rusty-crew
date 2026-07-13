export type ExternalRuntimeCommandName =
  | "help"
  | "commands"
  | "status"
  | "model"
  | "effort"
  | "compact";

export interface ParsedExternalRuntimeCommand {
  readonly input: string;
  readonly command: ExternalRuntimeCommandName;
  readonly argument: string | null;
}

export interface ExternalRuntimeCommandDefinition {
  readonly name: ExternalRuntimeCommandName;
  readonly aliases: readonly ExternalRuntimeCommandName[];
  readonly usage: string;
  readonly description: string;
  readonly mutates: boolean;
  readonly requiredCapabilities: readonly string[];
}

export const EXTERNAL_RUNTIME_COMMAND_DEFINITIONS = Object.freeze([
  {
    name: "help",
    aliases: ["commands"],
    usage: "/help",
    description: "List commands available for this external agent thread.",
    mutates: false,
    requiredCapabilities: [],
  },
  {
    name: "status",
    aliases: [],
    usage: "/status",
    description:
      "Report controller, binding, thread, active-turn, settings, and token usage state.",
    mutates: false,
    requiredCapabilities: [],
  },
  {
    name: "model",
    aliases: [],
    usage: "/model [id]",
    description:
      "Show the native model catalog or select a model for subsequent turns.",
    mutates: true,
    requiredCapabilities: ["model/list", "thread/settings/update"],
  },
  {
    name: "effort",
    aliases: [],
    usage: "/effort [value]",
    description:
      "Show or select a model-supported reasoning effort for subsequent turns.",
    mutates: true,
    requiredCapabilities: ["model/list", "thread/settings/update"],
  },
  {
    name: "compact",
    aliases: [],
    usage: "/compact",
    description: "Request native Codex compaction for the idle thread.",
    mutates: true,
    requiredCapabilities: ["thread/compact/start"],
  },
] as const satisfies readonly ExternalRuntimeCommandDefinition[]);

const commandNames = new Set<ExternalRuntimeCommandName>(
  EXTERNAL_RUNTIME_COMMAND_DEFINITIONS.flatMap((definition) => [
    definition.name,
    ...definition.aliases,
  ]),
);

export class ExternalRuntimeCommandInputError extends Error {
  constructor(
    readonly reasonCode:
      | "external_command_invalid_input"
      | "external_command_unknown",
    message: string,
  ) {
    super(message);
    this.name = "ExternalRuntimeCommandInputError";
  }
}

export function isRecognizedExternalRuntimeCommandInput(
  input: string,
): boolean {
  const command = commandToken(input);
  return command !== undefined && commandNames.has(command);
}

export function parseExternalRuntimeCommand(
  rawInput: string,
): ParsedExternalRuntimeCommand {
  const input = rawInput.trim();
  if (!input.startsWith("/")) {
    throw new ExternalRuntimeCommandInputError(
      "external_command_invalid_input",
      "external command input must start with /",
    );
  }
  const [rawCommand = "", ...argumentParts] = input.slice(1).split(/\s+/u);
  const command = rawCommand.toLowerCase();
  if (!commandNames.has(command as ExternalRuntimeCommandName)) {
    throw new ExternalRuntimeCommandInputError(
      "external_command_unknown",
      `external command /${rawCommand} is not recognized`,
    );
  }
  const argument = argumentParts.length === 0 ? null : argumentParts.join(" ");
  if (argument !== null && argument.length > 256) {
    throw new ExternalRuntimeCommandInputError(
      "external_command_invalid_input",
      "external command argument exceeds 256 characters",
    );
  }
  if (argument !== null && command !== "model" && command !== "effort") {
    throw new ExternalRuntimeCommandInputError(
      "external_command_invalid_input",
      `external command /${command} does not accept an argument`,
    );
  }
  return {
    input,
    command: command as ExternalRuntimeCommandName,
    argument,
  };
}

function commandToken(input: string): ExternalRuntimeCommandName | undefined {
  const match = /^\/([^\s]+)/u.exec(input.trim());
  if (match?.[1] === undefined) return undefined;
  return match[1].toLowerCase() as ExternalRuntimeCommandName;
}
