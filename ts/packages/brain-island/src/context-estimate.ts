export type ContextEstimateQuality = "exact" | "approximate" | "unavailable";

export interface ContextBudgetProvider {
  contextWindowTokens?: number;
  maxOutputTokens?: number;
}

export interface ContextTokenBudget {
  contextWindowTokens?: number;
  maxOutputTokens?: number;
  reservedResponseTokens?: number;
  safetyMarginTokens?: number;
  usableInputTokens?: number;
}

export interface ContextEstimateInput {
  provider?: ContextBudgetProvider;
  textFragments: readonly string[];
  sampledEventCount: number;
  sampledMessageCount: number;
}

export interface ContextUsageEstimate {
  estimateQuality: ContextEstimateQuality;
  estimateMethod: string;
  estimatorId: string;
  estimatedPromptTokens?: number;
  estimatedRemainingTokens?: number;
  budget: ContextTokenBudget;
  sampledEventCount: number;
  sampledMessageCount: number;
}

const FALLBACK_ESTIMATOR_ID = "fallback_chars_words_v1";
const FALLBACK_ESTIMATE_METHOD =
  "approximate_chars_div4_and_words_4over3_from_chat_events";
const DEFAULT_SAFETY_MARGIN_PERCENT = 2;

export function estimateContextUsage(
  input: ContextEstimateInput,
): ContextUsageEstimate {
  const budget = contextTokenBudget(input.provider);
  const sampledText = input.textFragments.join("\n");
  const estimatedPromptTokens =
    sampledText.trim().length === 0
      ? 0
      : estimateApproximateTokens(sampledText);
  return {
    estimateQuality:
      input.provider === undefined ? "unavailable" : "approximate",
    estimateMethod: FALLBACK_ESTIMATE_METHOD,
    estimatorId: FALLBACK_ESTIMATOR_ID,
    estimatedPromptTokens,
    estimatedRemainingTokens:
      budget.contextWindowTokens === undefined
        ? undefined
        : Math.max(0, budget.contextWindowTokens - estimatedPromptTokens),
    budget,
    sampledEventCount: input.sampledEventCount,
    sampledMessageCount: input.sampledMessageCount,
  };
}

export function contextTokenBudget(
  provider: ContextBudgetProvider | undefined,
): ContextTokenBudget {
  if (provider === undefined) {
    return {};
  }
  const contextWindowTokens = provider.contextWindowTokens;
  const maxOutputTokens = provider.maxOutputTokens;
  if (contextWindowTokens === undefined) {
    return {
      contextWindowTokens,
      maxOutputTokens,
      reservedResponseTokens: maxOutputTokens,
    };
  }
  const safetyMarginTokens = Math.max(
    0,
    Math.ceil((contextWindowTokens * DEFAULT_SAFETY_MARGIN_PERCENT) / 100),
  );
  const reservedResponseTokens = maxOutputTokens ?? 0;
  return {
    contextWindowTokens,
    maxOutputTokens,
    reservedResponseTokens,
    safetyMarginTokens,
    usableInputTokens: Math.max(
      0,
      contextWindowTokens - reservedResponseTokens - safetyMarginTokens,
    ),
  };
}

export function textFragmentsFromPayload(payload: unknown): string[] {
  if (!isRecord(payload)) return [];
  return [
    optionalString(payload.body),
    optionalString(payload.text),
    optionalString(payload.summary),
  ].filter((value): value is string => value !== undefined);
}

export function estimateApproximateTokens(text: string): number {
  const chars = Math.ceil(text.length / 4);
  const words = Math.ceil(
    text.trim().split(/\s+/).filter(Boolean).length * 1.33,
  );
  return Math.max(chars, words);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
