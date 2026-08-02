import type { ChatCompletionsTransportMetrics } from "./public-api.js";

export interface RawChatCompletionsTransportMetrics {
  provider_request_count: number;
  tool_round_count: number;
  provider_event_counts: Record<string, number>;
  provider_request_debug_samples?: unknown[];
  prompt_caching_policy: "disabled" | "automatic_5m" | "automatic_1h";
  openrouter_session_id?: string | null;
  prompt_tokens: number;
  cached_prompt_tokens: number;
  cache_write_prompt_tokens: number;
}

export function chatCompletionsTransportMetricsFromRaw(
  raw: RawChatCompletionsTransportMetrics | undefined,
): ChatCompletionsTransportMetrics | undefined {
  if (!raw) return undefined;
  return {
    effectiveTransport: "rust-chat-completions",
    selectedStrategyId: "default",
    effectiveStrategyId: "default",
    fallbackReason: null,
    providerRequestCount: raw.provider_request_count,
    continuationRoundCount: raw.tool_round_count,
    providerRequestPayloadBytes: 0,
    providerRequestDebugSamples: raw.provider_request_debug_samples ?? [],
    providerEventCounts: raw.provider_event_counts,
    promptCachingPolicy: raw.prompt_caching_policy,
    openrouterSessionId: raw.openrouter_session_id ?? undefined,
    promptTokens: raw.prompt_tokens,
    cachedPromptTokens: raw.cached_prompt_tokens,
    cacheWritePromptTokens: raw.cache_write_prompt_tokens,
    firstTextDeltaLatencyMs: null,
    totalTurnDurationMs: 0,
    toolRoundCount: raw.tool_round_count,
  };
}
