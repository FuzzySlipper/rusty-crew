export const DEFAULT_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS = 64;

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

export function responsesContinuationDiagnostics(moduleId: string): {
  workQuantumContinuationRounds?: number;
} {
  return moduleId === "openai-responses"
    ? {
        workQuantumContinuationRounds: responsesWorkQuantumContinuationRounds(),
      }
    : {};
}
