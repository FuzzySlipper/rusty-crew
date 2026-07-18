export const DEFAULT_RESPONSES_MAX_CONTINUATION_ROUNDS = 64;
export const MAX_RESPONSES_MAX_CONTINUATION_ROUNDS = 512;

export function responsesMaxContinuationRounds(
  env: Partial<NodeJS.ProcessEnv> = process.env,
): number {
  const variableName = "RUSTY_CREW_OPENAI_RESPONSES_MAX_CONTINUATION_ROUNDS";
  const normalized = env[variableName]?.trim();
  if (normalized === undefined || normalized === "") {
    return DEFAULT_RESPONSES_MAX_CONTINUATION_ROUNDS;
  }
  const configured = Number(normalized);
  if (
    Number.isSafeInteger(configured) &&
    configured > 0 &&
    configured <= MAX_RESPONSES_MAX_CONTINUATION_ROUNDS
  ) {
    return configured;
  }
  throw new Error(
    `${variableName} must be an integer between 1 and ${MAX_RESPONSES_MAX_CONTINUATION_ROUNDS}`,
  );
}

export function responsesContinuationDiagnostics(moduleId: string): {
  maxContinuationRounds?: number;
} {
  return moduleId === "openai-responses"
    ? { maxContinuationRounds: responsesMaxContinuationRounds() }
    : {};
}
