import type { BrainEvent, CoreEvent } from "@rusty-crew/contracts";
import type { CuratorObservedBehaviorEvidence } from "./curator-candidates.js";

export type PostTurnMaintenanceDecision =
  | {
      action: "noop";
      reasonCode:
        | "background_review_disabled"
        | "turn_not_complex"
        | "completion_unavailable";
      summary: string;
    }
  | {
      action: "propose_skill_candidate";
      evidence: CuratorObservedBehaviorEvidence;
    };

export interface PostTurnMaintenanceInput {
  profileId: string;
  wakeId: string;
  source: "background" | "direct_debug" | "delivery" | "chat";
  backgroundReviewEnabled: boolean;
  events: readonly CoreEvent[];
  completionSummary?: string;
}

export function postTurnMaintenanceDecision(
  input: PostTurnMaintenanceInput,
): PostTurnMaintenanceDecision {
  if (!input.backgroundReviewEnabled) {
    return {
      action: "noop",
      reasonCode: "background_review_disabled",
      summary: "post-turn maintenance skipped because background review is off",
    };
  }
  if (!input.completionSummary?.trim()) {
    return {
      action: "noop",
      reasonCode: "completion_unavailable",
      summary:
        "post-turn maintenance skipped because no completion summary was available",
    };
  }

  const toolCalls = observedToolStarts(input.events, input.wakeId);
  const complex =
    toolCalls.length >= 3 || input.completionSummary.trim().length >= 1_000;
  if (!complex) {
    return {
      action: "noop",
      reasonCode: "turn_not_complex",
      summary:
        "post-turn maintenance made no candidate because the turn was not complex enough",
    };
  }

  const slug = `${safeSlug(input.profileId)}-observed-workflow`;
  return {
    action: "propose_skill_candidate",
    evidence: {
      evidenceId: `post-turn:${input.wakeId}`,
      summary: `${input.profileId} completed a reusable ${input.source} workflow`,
      suggestedSkillSlug: slug,
      suggestedTitle: titleFromSlug(slug),
      suggestedSummary:
        "A reusable workflow observed from a complex Rusty Crew turn.",
      workflowMarkdown: workflowMarkdown(input, toolCalls),
      occurrences: 1,
      confidence: toolCalls.length >= 3 ? 0.74 : 0.68,
      tags: ["post-turn", "auto-maintenance"],
      sourceRefs: [
        {
          kind: "observed_behavior",
          ref: `wake:${input.wakeId}`,
        },
      ],
    },
  };
}

function observedToolStarts(
  events: readonly CoreEvent[],
  wakeId: string,
): string[] {
  return events.flatMap((event) => {
    if (event.type !== "brain_event_observed") return [];
    if (event.wakeId !== wakeId) return [];
    const brainEvent = event.event as BrainEvent;
    return brainEvent.type === "tool_call_started" ? [brainEvent.toolName] : [];
  });
}

function workflowMarkdown(
  input: PostTurnMaintenanceInput,
  toolCalls: readonly string[],
): string {
  return [
    `Observed wake: ${input.wakeId}`,
    `Source: ${input.source}`,
    `Tools started: ${toolCalls.length > 0 ? toolCalls.join(", ") : "none"}`,
    "",
    "Completion summary:",
    input.completionSummary?.trim() ?? "",
  ].join("\n");
}

function safeSlug(value: string): string {
  return (
    value
      .trim()
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 72)
      .replace(/-+$/g, "") || "profile"
  );
}

function titleFromSlug(slug: string): string {
  return slug
    .split("-")
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ""}${part.slice(1)}`)
    .join(" ");
}
