import type { BrainEventEnvelope } from "@rusty-crew/contracts";
import type { BrainWakeInput } from "./index.js";

export function providerRequestDebugEvent(
  wake: BrainWakeInput,
  debug: {
    debug_detail_id: string;
    request_sha256: string;
    request_json_chars: number;
    expires_at: string;
  },
): BrainEventEnvelope {
  return {
    wakeId: wake.wakeId,
    sessionId: wake.sessionId,
    event: {
      type: "provider_status",
      level: "info",
      message: "Provider request debug snapshot captured.",
      metadataJson: JSON.stringify({
        provider_request_debug_detail_id: debug.debug_detail_id,
        provider_request_debug_url: `/v1/chat/sessions/${encodeURIComponent(
          String(wake.sessionId),
        )}/provider-requests/${encodeURIComponent(debug.debug_detail_id)}`,
        request_sha256: debug.request_sha256,
        request_json_chars: debug.request_json_chars,
        expires_at: debug.expires_at,
      }),
    },
  };
}
