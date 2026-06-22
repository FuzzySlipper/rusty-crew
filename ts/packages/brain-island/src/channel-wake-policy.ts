export type ChannelWakePolicy = "subscription" | "manual" | "disabled";

export type DeliveryIntentWakeDecision =
  | {
      action: "claim_and_wake";
      wakePolicy: ChannelWakePolicy;
    }
  | {
      action: "manual_wait";
      wakePolicy: ChannelWakePolicy;
      reasonCode: "wake_policy_manual";
      summary: string;
    }
  | {
      action: "reject";
      wakePolicy: ChannelWakePolicy;
      reasonCode: "wake_policy_disabled";
      summary: string;
    }
  | {
      action: "skip_expired";
      wakePolicy: ChannelWakePolicy;
      reasonCode: "delivery_intent_expired";
      summary: string;
    };

export function normalizeChannelWakePolicy(
  value: string | undefined,
): ChannelWakePolicy {
  if (value === "manual" || value === "disabled") return value;
  return "subscription";
}

export function deliveryIntentWakeDecision(input: {
  wakePolicy: string | undefined;
  expiresAt: string;
  now: string;
}): DeliveryIntentWakeDecision {
  const wakePolicy = normalizeChannelWakePolicy(input.wakePolicy);
  if (deliveryIntentExpired(input.expiresAt, input.now)) {
    return {
      action: "skip_expired",
      wakePolicy,
      reasonCode: "delivery_intent_expired",
      summary: "delivery intent expired before Rusty Crew claimed it",
    };
  }
  switch (wakePolicy) {
    case "manual":
      return {
        action: "manual_wait",
        wakePolicy,
        reasonCode: "wake_policy_manual",
        summary:
          "profile channel wake policy is manual; waiting for an explicit pull/claim path",
      };
    case "disabled":
      return {
        action: "reject",
        wakePolicy,
        reasonCode: "wake_policy_disabled",
        summary: "profile channel wake policy disables automatic delivery",
      };
    case "subscription":
      return { action: "claim_and_wake", wakePolicy };
  }
}

function deliveryIntentExpired(expiresAtValue: string, nowValue: string): boolean {
  const expiresAt = Date.parse(expiresAtValue);
  const now = Date.parse(nowValue);
  return (
    Number.isFinite(expiresAt) &&
    Number.isFinite(now) &&
    expiresAt <= now
  );
}
