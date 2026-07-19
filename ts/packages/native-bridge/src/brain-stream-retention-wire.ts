import type { NativeBufferedBrainStreamRetentionMetrics } from "./public-api.js";

export interface RawBufferedBrainStreamRetentionMetrics {
  raw_stream_item_count: number;
  raw_delta_item_count: number;
  retained_stream_item_count: number;
  coalesced_delta_item_count: number;
  dropped_stream_item_count: number;
  retained_delta_bytes: number;
  queued_delta_bytes: number;
  max_stream_items: number;
  max_stream_delta_bytes: number;
}

export function decodeStreamRetention(
  raw: RawBufferedBrainStreamRetentionMetrics,
): NativeBufferedBrainStreamRetentionMetrics {
  return {
    rawStreamItemCount: raw.raw_stream_item_count,
    rawDeltaItemCount: raw.raw_delta_item_count,
    retainedStreamItemCount: raw.retained_stream_item_count,
    coalescedDeltaItemCount: raw.coalesced_delta_item_count,
    droppedStreamItemCount: raw.dropped_stream_item_count,
    retainedDeltaBytes: raw.retained_delta_bytes,
    queuedDeltaBytes: raw.queued_delta_bytes,
    maxStreamItems: raw.max_stream_items,
    maxStreamDeltaBytes: raw.max_stream_delta_bytes,
  };
}

export function encodeStreamRetention(
  metrics: NativeBufferedBrainStreamRetentionMetrics,
): RawBufferedBrainStreamRetentionMetrics {
  return {
    raw_stream_item_count: metrics.rawStreamItemCount,
    raw_delta_item_count: metrics.rawDeltaItemCount,
    retained_stream_item_count: metrics.retainedStreamItemCount,
    coalesced_delta_item_count: metrics.coalescedDeltaItemCount,
    dropped_stream_item_count: metrics.droppedStreamItemCount,
    retained_delta_bytes: metrics.retainedDeltaBytes,
    queued_delta_bytes: metrics.queuedDeltaBytes,
    max_stream_items: metrics.maxStreamItems,
    max_stream_delta_bytes: metrics.maxStreamDeltaBytes,
  };
}
