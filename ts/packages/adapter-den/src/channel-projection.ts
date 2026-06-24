import type {
  AgentId,
  AgentMessage,
  ChannelBindingRecord,
  ChannelSeverity,
  CoreEvent,
  NormalizedChannelActivityProjection,
  NormalizedChannelOutboundMessage,
} from "@rusty-crew/contracts";
import { providerRefsFromBinding } from "./channel-presence.js";

const DEFAULT_MAX_BODY_CHARS = 3_500;
const DEFAULT_MAX_SUMMARY_CHARS = 480;

export interface ChannelProjectionOptions {
  maxBodyChars?: number;
  maxSummaryChars?: number;
  now?: string;
}

export type ChannelOutboundProjectionResult =
  | {
      status: "projected";
      message: NormalizedChannelOutboundMessage;
      binding: ChannelBindingRecord;
    }
  | {
      status: "not_channel_target" | "no_binding" | "inactive_binding";
      reason: string;
      candidates: ChannelBindingRecord[];
    };

export interface ChannelProjectionSink {
  sendMessage(message: NormalizedChannelOutboundMessage): Promise<void> | void;
  sendActivity(
    activity: NormalizedChannelActivityProjection,
  ): Promise<void> | void;
}

export type ChannelProjectionDispatchResult =
  | { accepted: true; kind: "message" | "activity" }
  | {
      accepted: false;
      kind: "message" | "activity";
      degradedReason: string;
    };

export function projectAgentMessageToChannel(
  message: AgentMessage,
  bindings: readonly ChannelBindingRecord[],
  options: ChannelProjectionOptions = {},
): ChannelOutboundProjectionResult {
  const correlationBindingId = bindingIdFromCorrelation(message.correlationId);
  const targetBindingId = bindingIdFromChannelTarget(message.to);
  const bindingId = correlationBindingId ?? targetBindingId;

  if (bindingId === undefined && !isChannelTarget(message.to)) {
    return {
      status: "not_channel_target",
      reason: "agent message target is not a channel address",
      candidates: [],
    };
  }

  const candidates =
    bindingId === undefined
      ? bindings.filter((binding) => binding.agentId === message.from)
      : bindings.filter((binding) => binding.bindingId === bindingId);
  const active = candidates.filter((binding) => binding.status === "active");

  if (active.length === 0) {
    return {
      status: candidates.length > 0 ? "inactive_binding" : "no_binding",
      reason:
        candidates.length > 0
          ? "matching channel binding is not active"
          : "no channel binding matches outbound message",
      candidates,
    };
  }

  const binding = active[0]!;
  return {
    status: "projected",
    binding,
    message: {
      kind: "channel_outbound_message.v1",
      adapterId: binding.adapterId,
      bindingId: binding.bindingId,
      runtime: {
        agentId: binding.agentId,
        instanceId: binding.instanceId,
        sessionId: binding.sessionId,
        profileId: binding.profileId,
      },
      providerRefs: providerRefsFromBinding(binding),
      body: boundedText(message.body, options.maxBodyChars),
      correlationId: message.correlationId,
      idempotencyKey: outboundIdempotencyKey(binding, message),
      visibility: "conversation",
      deliveryPolicy: "best_effort",
    },
  };
}

export function projectCoreEventToChannelActivity(
  event: CoreEvent,
  binding: ChannelBindingRecord,
  options: ChannelProjectionOptions = {},
): NormalizedChannelActivityProjection {
  const summary = activitySummary(event);
  return {
    kind: "channel_activity_projection.v1",
    adapterId: binding.adapterId,
    bindingId: binding.bindingId,
    runtime: {
      agentId: binding.agentId,
      instanceId: binding.instanceId,
      sessionId: binding.sessionId,
      profileId: binding.profileId,
    },
    providerRefs: providerRefsFromBinding(binding),
    eventType: event.type,
    summary: boundedText(summary.text, options.maxSummaryChars),
    severity: summary.severity,
    workRef: workRefForEvent(event),
    resultRef: resultRefForEvent(event),
    createdAt: options.now ?? new Date().toISOString(),
  };
}

export async function dispatchChannelMessageProjection(
  sink: ChannelProjectionSink,
  message: NormalizedChannelOutboundMessage,
): Promise<ChannelProjectionDispatchResult> {
  try {
    await sink.sendMessage(message);
    return { accepted: true, kind: "message" };
  } catch (error) {
    return {
      accepted: false,
      kind: "message",
      degradedReason: projectionErrorMessage(error),
    };
  }
}

export async function dispatchChannelActivityProjection(
  sink: ChannelProjectionSink,
  activity: NormalizedChannelActivityProjection,
): Promise<ChannelProjectionDispatchResult> {
  try {
    await sink.sendActivity(activity);
    return { accepted: true, kind: "activity" };
  } catch (error) {
    return {
      accepted: false,
      kind: "activity",
      degradedReason: projectionErrorMessage(error),
    };
  }
}

function isChannelTarget(agentId: AgentId): boolean {
  return agentId.startsWith("channel:");
}

function bindingIdFromChannelTarget(agentId: AgentId): string | undefined {
  const [, targetKind, bindingId] = agentId.split(":");
  return targetKind === "binding" && bindingId ? bindingId : undefined;
}

function bindingIdFromCorrelation(
  correlationId: string | undefined,
): string | undefined {
  const match = correlationId?.match(/^channel:([^:]+):/);
  return match?.[1];
}

function boundedText(value: string, maxChars = DEFAULT_MAX_BODY_CHARS): string {
  const trimmed = value.trim();
  if (trimmed.length <= maxChars) return trimmed;
  const marker = "\n[truncated]";
  return `${trimmed.slice(0, Math.max(0, maxChars - marker.length))}${marker}`;
}

function outboundIdempotencyKey(
  binding: ChannelBindingRecord,
  message: AgentMessage,
): string {
  const correlation = message.correlationId ?? "uncorrelated";
  return `channel_outbound:${binding.bindingId}:${correlation}:${stableTextKey(message.body)}`;
}

function stableTextKey(value: string): string {
  let hash = 0;
  for (const char of value) {
    hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  }
  return hash.toString(16).padStart(8, "0");
}

function activitySummary(event: CoreEvent): {
  text: string;
  severity: ChannelSeverity;
} {
  switch (event.type) {
    case "session_created":
      return {
        text: `Session created for ${event.state.agentId}`,
        severity: "info",
      };
    case "session_archived":
      return {
        text: `Session archived ${event.sessionId}`,
        severity: "warning",
      };
    case "agent_message_routed":
      return {
        text: `Message routed ${event.message.from} -> ${event.message.to}`,
        severity: "info",
      };
    case "delegation_lifecycle_observed":
      return {
        text: `Delegation ${event.lifecycle.phase} for ${event.lifecycle.delegatedSessionId}`,
        severity:
          event.lifecycle.phase === "failed" ||
          event.lifecycle.phase === "blocked" ||
          event.lifecycle.phase === "timed_out"
            ? "error"
            : "info",
      };
    case "external_event_injected":
      return {
        text: `External event accepted from ${event.event.source}`,
        severity: "info",
      };
    case "den_data_updated":
      return {
        text: `Den ${event.update.entityKind} updated ${event.update.entityId}`,
        severity: "info",
      };
    case "brain_wake_requested":
      return {
        text: `Brain wake requested for ${event.sessionId}`,
        severity: "info",
      };
    case "brain_event_observed":
      return {
        text: brainEventSummary(event),
        severity:
          event.event.type === "tool_call_finished" && event.event.isError
            ? "error"
            : event.event.type === "provider_status" &&
                event.event.level === "error"
              ? "error"
              : event.event.type === "provider_status" &&
                  event.event.level === "degraded"
                ? "warning"
                : "info",
      };
    case "brain_actions_accepted":
      return {
        text: `Brain accepted ${event.count} actions for ${event.sessionId}`,
        severity: "success",
      };
    case "completion_packet_delivered":
      return {
        text: `Completion ${event.packet.status} for ${event.packet.sessionId}`,
        severity:
          event.packet.status === "completed"
            ? "success"
            : event.packet.status === "failed"
              ? "error"
              : "warning",
      };
  }
}

function brainEventSummary(
  event: Extract<CoreEvent, { type: "brain_event_observed" }>,
): string {
  switch (event.event.type) {
    case "started":
      return `Brain started for ${event.sessionId}`;
    case "text_delta":
      return `Brain produced text for ${event.sessionId}`;
    case "tool_call_started":
      return `Tool started: ${event.event.toolName}`;
    case "tool_call_finished":
      return `Tool finished: ${event.event.toolName}`;
    case "provider_status":
      return `Provider ${event.event.level}: ${event.event.message}`;
    case "finished":
      return `Brain finished for ${event.sessionId}`;
  }
}

function workRefForEvent(event: CoreEvent): string | undefined {
  if (event.type === "delegation_lifecycle_observed" && event.lifecycle.runId) {
    return `run:${event.lifecycle.runId}`;
  }
  return undefined;
}

function resultRefForEvent(event: CoreEvent): string | undefined {
  if (event.type === "completion_packet_delivered") {
    return `completion:${event.packet.sessionId}:${event.packet.status}`;
  }
  return undefined;
}

function projectionErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
