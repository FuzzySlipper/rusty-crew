export const DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS = 64;
export const MAX_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS = 512;

export function chatCompletionsMaxToolRounds(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName = "RUSTY_CREW_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS;
  }
  const configured = Number(normalized);
  if (
    Number.isSafeInteger(configured) &&
    configured > 0 &&
    configured <= MAX_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS
  ) {
    return configured;
  }
  throw new Error(
    `${variableName} must be an integer between 1 and ${MAX_CHAT_COMPLETIONS_MAX_TOOL_ROUNDS}`,
  );
}

export function chatCompletionsContinuationDiagnostics(moduleId: string): {
  maxContinuationRounds?: number;
} {
  return moduleId === "chat-completions"
    ? { maxContinuationRounds: chatCompletionsMaxToolRounds() }
    : {};
}
