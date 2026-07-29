export const DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS = 64;
export const DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD = 3;

export function chatCompletionsWorkQuantumToolRounds(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName = "RUSTY_CREW_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_CHAT_COMPLETIONS_WORK_QUANTUM_TOOL_ROUNDS;
  }
  const configured = Number(normalized);
  if (Number.isSafeInteger(configured) && configured > 0) {
    return configured;
  }
  throw new Error(`${variableName} must be a positive safe integer`);
}

export function chatCompletionsNoProgressAttentionThreshold(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName =
    "RUSTY_CREW_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_CHAT_COMPLETIONS_NO_PROGRESS_ATTENTION_THRESHOLD;
  }
  const configured = Number(normalized);
  if (Number.isSafeInteger(configured) && configured >= 2) {
    return configured;
  }
  throw new Error(`${variableName} must be a safe integer of at least 2`);
}

export function chatCompletionsContinuationDiagnostics(moduleId: string): {
  workQuantumToolRounds?: number;
  noProgressAttentionThreshold?: number;
} {
  return moduleId === "chat-completions"
    ? {
        workQuantumToolRounds: chatCompletionsWorkQuantumToolRounds(),
        noProgressAttentionThreshold:
          chatCompletionsNoProgressAttentionThreshold(),
      }
    : {};
}
