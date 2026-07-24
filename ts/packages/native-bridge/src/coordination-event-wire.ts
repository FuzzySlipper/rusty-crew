import type { CoreEvent } from "@rusty-crew/contracts";

export type CoordinationObservedCoreEvent = Extract<
  CoreEvent,
  { type: "agent_message_delivery_observed" | "agent_round_observed" }
>;

export type RawCoordinationObservedCoreEvent =
  | {
      type: "agent_message_delivery_observed";
      receipt: Extract<
        CoreEvent,
        { type: "agent_message_delivery_observed" }
      >["receipt"];
    }
  | {
      type: "agent_round_observed";
      round: Extract<CoreEvent, { type: "agent_round_observed" }>["round"];
    };

export function toNativeCoordinationObservedCoreEvent(
  event: CoordinationObservedCoreEvent,
): RawCoordinationObservedCoreEvent {
  switch (event.type) {
    case "agent_message_delivery_observed":
      return { type: event.type, receipt: event.receipt };
    case "agent_round_observed":
      return { type: event.type, round: event.round };
  }
}

export function toCoordinationObservedCoreEvent(
  event: RawCoordinationObservedCoreEvent,
): CoordinationObservedCoreEvent {
  switch (event.type) {
    case "agent_message_delivery_observed":
      return { type: event.type, receipt: event.receipt };
    case "agent_round_observed":
      return { type: event.type, round: event.round };
  }
}
