import type {
  AgentMessage,
  ChannelBindingRecord,
  EventReceipt,
  ExternalEvent,
  NormalizedChannelInboundMessage,
} from "@rusty-crew/contracts";
import {
  denChannelsInboundToChannelExternalEvent,
  isExpiredChannelInboundMessage,
} from "./den-channels.js";
import type { DenChannelsInboundDecision } from "./den-channel-transport.js";
import type { ChannelRoutingOptions } from "./channel-routing.js";
import { resolveChannelRoute } from "./channel-routing.js";

export interface ChannelIngressBridge {
  injectExternalEvent(
    event: ExternalEvent,
  ): Promise<EventReceipt> | EventReceipt;
  routeAgentMessage(
    message: AgentMessage,
  ): Promise<EventReceipt> | EventReceipt;
}

export type ChannelIngressResult =
  | {
      status: "routed";
      message: NormalizedChannelInboundMessage;
      externalEvent: ExternalEvent;
      externalReceipt: EventReceipt;
      routedMessage: AgentMessage;
      routeReceipt: EventReceipt;
    }
  | {
      status:
        | "expired"
        | "duplicate"
        | "stale_cursor"
        | "no_binding"
        | "ambiguous"
        | "inactive_binding";
      reason: string;
      message: NormalizedChannelInboundMessage;
      candidates?: ChannelBindingRecord[];
    };

export interface ChannelIngressOptions {
  bridge: ChannelIngressBridge;
  bindings: readonly ChannelBindingRecord[];
  now?: string;
  routing?: ChannelRoutingOptions;
}

export async function ingestAcceptedChannelDecision(
  decision: DenChannelsInboundDecision,
  options: ChannelIngressOptions,
): Promise<ChannelIngressResult> {
  if (!decision.accepted) {
    return {
      status: decision.reason,
      reason: `transport rejected inbound channel message as ${decision.reason}`,
      message: decision.message,
    };
  }

  return ingestChannelInboundMessage(decision.message, options);
}

export async function ingestChannelInboundMessage(
  message: NormalizedChannelInboundMessage,
  options: ChannelIngressOptions,
): Promise<ChannelIngressResult> {
  if (isExpiredChannelInboundMessage(message, options.now)) {
    return {
      status: "expired",
      reason: "channel message expired before Rust ingress",
      message,
    };
  }

  const resolution = resolveChannelRoute(
    message,
    options.bindings,
    options.routing,
  );
  if (resolution.status !== "routed") {
    return {
      status: resolution.status,
      reason: resolution.reason,
      message,
      candidates: resolution.candidates,
    };
  }

  const externalEvent = denChannelsInboundToChannelExternalEvent(
    message,
    resolution.route.correlationId,
  );
  const externalReceipt =
    await options.bridge.injectExternalEvent(externalEvent);
  const routedMessage: AgentMessage = {
    from: resolution.route.from,
    to: resolution.route.to,
    body: resolution.route.body,
    correlationId: resolution.route.correlationId,
  };
  const routeReceipt = await options.bridge.routeAgentMessage(routedMessage);

  return {
    status: "routed",
    message,
    externalEvent,
    externalReceipt,
    routedMessage,
    routeReceipt,
  };
}
