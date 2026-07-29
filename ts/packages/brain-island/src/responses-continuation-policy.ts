export const DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS = 64;
export const DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD = 3;

export function responsesWorkQuantumContinuationRounds(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName =
    "RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS;
  }
  const configured = Number(normalized);
  if (Number.isSafeInteger(configured) && configured > 0) {
    return configured;
  }
  throw new Error(`${variableName} must be a positive safe integer`);
}

export function responsesNoProgressAttentionThreshold(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName =
    "RUSTY_CREW_OPENAI_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_RESPONSES_NO_PROGRESS_ATTENTION_THRESHOLD;
  }
  const configured = Number(normalized);
  if (Number.isSafeInteger(configured) && configured >= 2) {
    return configured;
  }
  throw new Error(`${variableName} must be a safe integer of at least 2`);
}

export function responsesContinuationDiagnostics(moduleId: string): {
  workQuantumContinuationRounds?: number;
  noProgressAttentionThreshold?: number;
} {
  return moduleId === "openai-responses"
    ? {
        workQuantumContinuationRounds: responsesWorkQuantumContinuationRounds(),
        noProgressAttentionThreshold: responsesNoProgressAttentionThreshold(),
      }
    : {};
}
