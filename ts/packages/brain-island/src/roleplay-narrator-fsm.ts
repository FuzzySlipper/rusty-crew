import type { BrainPhase } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export type RoleplayNarratorPhaseKind =
  | "prelude_explore"
  | "prelude_capture"
  | "explore"
  | "compose"
  | "compose_draft"
  | "review"
  | "done";

export type RoleplayNarratorProviderPhase =
  | "explore"
  | "compose"
  | "compose_draft"
  | "review";

export type RoleplayNarratorJsonValue =
  | null
  | boolean
  | number
  | string
  | RoleplayNarratorJsonValue[]
  | { [key: string]: RoleplayNarratorJsonValue };

export interface RoleplayNarratorReviewConfig {
  enabled: boolean;
  maxReviewCycles: number;
}

export interface RoleplayNarratorConfig {
  tone: string;
  pacing: string;
  explicitness: string;
  memoryDepth: string;
  stylePrompt?: string;
  exemplar?: string;
  review: RoleplayNarratorReviewConfig;
}

export interface RoleplayNarratorToolRequest {
  toolName: string;
  paramsJson: RoleplayNarratorJsonValue;
}

export interface RoleplayNarratorToolObservation {
  toolName: string;
  ok: boolean;
  summary: string;
  detailsJson?: RoleplayNarratorJsonValue;
}

export interface RoleplayNarratorPromptSourceText {
  source_kind: string;
  source_id: string;
  title: string;
  body: string;
  editable: boolean;
  derived: boolean;
}

export interface RoleplayNarratorTurnState {
  profileId: string;
  sessionId: string;
  pendingText: string;
  narratorConfig?: RoleplayNarratorConfig;
  reviewEnabled: boolean;
  maxReviewCycles: number;
  reviewCycle: number;
  preludeObservations: RoleplayNarratorToolObservation[];
  relevantLore: RoleplayNarratorPromptSourceText[];
  sceneBrief?: string;
  reviewFeedback?: string;
  completedPhases: RoleplayNarratorPhaseKind[];
}

export interface RoleplayNarratorActivity {
  phase: Extract<BrainPhase, "exploring" | "composing" | "reviewing" | "idle">;
  message: string;
}

export type RoleplayNarratorDirective =
  | {
      kind: "tool_batch";
      requests: RoleplayNarratorToolRequest[];
    }
  | {
      kind: "provider_phase";
      phase: RoleplayNarratorProviderPhase;
      instructions: string;
      allowedTools: string[];
      outputMode: "internal" | "final";
    }
  | {
      kind: "done";
    };

export interface RoleplayNarratorTurnReceipt {
  receiptId: string;
  wakeId: string;
  sessionId: string;
  sequence: number;
  phase: RoleplayNarratorPhaseKind;
  activity?: RoleplayNarratorActivity;
  directive: RoleplayNarratorDirective;
  state: RoleplayNarratorTurnState;
  terminal: boolean;
}

export interface RoleplayNarratorStartInput {
  wakeId: string;
  sessionId: string;
  profileId: string;
  pendingText?: string;
  narratorConfig?: RoleplayNarratorConfig;
  reviewEnabled: boolean;
  maxReviewCycles?: number;
}

export type RoleplayNarratorPhaseOutcome =
  | {
      kind: "tool_batch_completed";
      observations: RoleplayNarratorToolObservation[];
    }
  | {
      kind: "provider_phase_completed";
      outputText?: string;
    };

export interface RoleplayNarratorAdvanceInput {
  receipt: RoleplayNarratorTurnReceipt;
  outcome: RoleplayNarratorPhaseOutcome;
}

export interface RoleplayNarratorFsmBridge {
  startTurn(
    input: RoleplayNarratorStartInput,
  ): Promise<RoleplayNarratorTurnReceipt>;
  advanceTurn(
    input: RoleplayNarratorAdvanceInput,
  ): Promise<RoleplayNarratorTurnReceipt>;
}

export function createRoleplayNarratorFsmBridge(
  bridge: Pick<
    NativeBridgeModule,
    "startRoleplayNarratorTurn" | "advanceRoleplayNarratorTurn"
  >,
): RoleplayNarratorFsmBridge {
  return {
    startTurn: async (input) =>
      (await bridge.startRoleplayNarratorTurn(
        input,
      )) as RoleplayNarratorTurnReceipt,
    advanceTurn: async (input) =>
      (await bridge.advanceRoleplayNarratorTurn(
        input,
      )) as RoleplayNarratorTurnReceipt,
  };
}
