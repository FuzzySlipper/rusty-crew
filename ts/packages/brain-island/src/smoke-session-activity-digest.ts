import assert from "node:assert/strict";
import type {
  AdapterId,
  CoreEvent,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { buildSessionActivityDigest } from "./session-activity-digest.js";

const profileId = "runner" as ProfileId;
const sessionId = "session-1" as SessionId;
const wakeId = "wake-1";

const textOnly = buildSessionActivityDigest({
  profileId,
  sessionId,
  wakeId,
  source: "direct_debug",
  now: "2026-06-27T12:00:00.000Z",
  events: [
    observed({ type: "started" }),
    observed({ type: "text_delta", text: "Hello" }),
    observed({ type: "text_delta", text: "Hello world" }),
    observed({ type: "finished" }),
  ],
  completionSummary: "Hello world",
});
assert.equal(textOnly.digest_id.length, 28);
assert.equal(textOnly.allowed_capture_spaces[0], "profile_dense");
assert.match(textOnly.summary_text, /Assistant text: Hello world/);
assert.equal(textOnly.event_counts_json["brain_event_observed.text_delta"], 2);
assert.equal(textOnly.signals_json.length, 1);

const tools = buildSessionActivityDigest({
  profileId,
  sessionId,
  wakeId,
  source: "channel",
  now: "2026-06-27T12:01:00.000Z",
  events: [
    observed({ type: "tool_call_started", toolName: "den_memory_recall" }),
    observed({
      type: "tool_call_finished",
      toolName: "den_memory_recall",
      isError: false,
    }),
    observed({ type: "tool_call_started", toolName: "shell" }),
    observed({ type: "tool_call_finished", toolName: "shell", isError: true }),
  ],
});
assert.deepEqual(
  tools.tool_calls_json.map((call) =>
    isToolCall(call) ? `${call.tool_name}:${call.status}` : "bad",
  ),
  [
    "den_memory_recall:started",
    "den_memory_recall:completed",
    "shell:started",
    "shell:failed",
  ],
);
assert.equal(
  tools.signals_json.some(
    (signal) => isSignal(signal) && signal.signal_type === "tool_failure",
  ),
  true,
);

const providerAndCorrection = buildSessionActivityDigest({
  profileId,
  sessionId,
  wakeId,
  source: "channel",
  now: "2026-06-27T12:02:00.000Z",
  events: [
    {
      type: "external_event_injected",
      event: {
        adapterId: "adapter" as AdapterId,
        source: "message-1",
        payload: {
          type: "human_message",
          from: "operator",
          text: "Actually the database lives on den-srv, not here.",
        },
      },
    },
    observed({
      type: "provider_status",
      level: "degraded",
      message: "provider retry used",
    }),
  ],
});
assert.equal(
  providerAndCorrection.signals_json.some(
    (signal) => isSignal(signal) && signal.signal_type === "user_correction",
  ),
  true,
);
assert.equal(
  providerAndCorrection.signals_json.some(
    (signal) => isSignal(signal) && signal.signal_type === "provider_status",
  ),
  true,
);

const truncated = buildSessionActivityDigest({
  profileId,
  sessionId,
  wakeId,
  source: "direct_debug",
  now: "2026-06-27T12:03:00.000Z",
  maxSummaryChars: 80,
  maxTextChars: 60,
  events: [observed({ type: "text_delta", text: "x".repeat(500) })],
});
assert.equal(truncated.summary_text.length, 80);
assert.equal(truncated.summary_text.endsWith("..."), true);

console.log("smoke-session-activity-digest ok");

function observed(
  event: Extract<CoreEvent, { type: "brain_event_observed" }>["event"],
): CoreEvent {
  return {
    type: "brain_event_observed",
    sessionId,
    wakeId,
    event,
  };
}

function isToolCall(
  value: unknown,
): value is { tool_name: string; status: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "tool_name" in value &&
    "status" in value
  );
}

function isSignal(
  value: unknown,
): value is { signal_type: string; severity: string } {
  return (
    typeof value === "object" &&
    value !== null &&
    "signal_type" in value &&
    "severity" in value
  );
}
