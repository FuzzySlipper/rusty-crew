import type { ChatCompletionsTransportMetrics } from "./public-api.js";

export interface RawChatCompletionsTransportMetrics {
  provider_request_count: number;
  tool_round_count: number;
  provider_event_counts: Record<string, number>;
  provider_request_debug_samples?: unknown[];
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
    firstTextDeltaLatencyMs: null,
    totalTurnDurationMs: 0,
    toolRoundCount: raw.tool_round_count,
  };
}
