import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export type RoleplayNarratorPhaseKind =
  | "explore"
  | "compose"
  | "compose_draft"
  | "review"
  | "done";

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

export interface RoleplayNarratorTurnState {
  narratorConfig?: RoleplayNarratorConfig;
  reviewEnabled: boolean;
  maxReviewCycles: number;
  reviewCycle: number;
  relevantLore: RoleplayNarratorPromptSourceText[];
  sceneBrief?: string;
  reviewFeedback?: string;
}

export interface RoleplayNarratorPromptSourceText {
  source_kind: string;
  source_id: string;
  title: string;
  body: string;
  editable: boolean;
  derived: boolean;
}

export interface RoleplayNarratorPhasePlan {
  phase: RoleplayNarratorPhaseKind;
  instructions: string;
  allowedTools: string[];
  mandatoryToolRequests: RoleplayNarratorToolRequest[];
  state: RoleplayNarratorTurnState;
  terminal: boolean;
}

export interface RoleplayNarratorMandatoryExploreInput {
  sessionId: string;
  profileId: string;
  pendingText?: string;
}

export interface RoleplayNarratorAutoCaptureInput {
  sessionId: string;
  profileId: string;
  wakeId: string;
  pendingText?: string;
  layerDetailsJson?: RoleplayNarratorJsonValue;
}

export interface RoleplayNarratorStartInput {
  narratorConfig?: RoleplayNarratorConfig;
  reviewEnabled: boolean;
  maxReviewCycles?: number;
  preludeObservations?: RoleplayNarratorToolObservation[];
}

export interface RoleplayNarratorNextInput {
  state: RoleplayNarratorTurnState;
  completedPhase: RoleplayNarratorPhaseKind;
  outputText?: string;
}

export interface RoleplayNarratorFsmBridge {
  mandatoryExploreRequests(
    input: RoleplayNarratorMandatoryExploreInput,
  ): Promise<RoleplayNarratorToolRequest[]>;
  autoCaptureRequest(
    input: RoleplayNarratorAutoCaptureInput,
  ): Promise<RoleplayNarratorToolRequest | undefined>;
  startTurn(
    input: RoleplayNarratorStartInput,
  ): Promise<RoleplayNarratorPhasePlan>;
  nextPhase(
    input: RoleplayNarratorNextInput,
  ): Promise<RoleplayNarratorPhasePlan>;
  reviewRequestsRevision(feedback: string): Promise<boolean>;
}

export function createRoleplayNarratorFsmBridge(
  bridge: Pick<
    NativeBridgeModule,
    | "roleplayNarratorMandatoryExploreRequests"
    | "roleplayNarratorAutoCaptureRequest"
    | "startRoleplayNarratorTurn"
    | "nextRoleplayNarratorPhase"
    | "roleplayNarratorReviewRequestsRevision"
  >,
): RoleplayNarratorFsmBridge {
  return {
    mandatoryExploreRequests: async (input) =>
      (await bridge.roleplayNarratorMandatoryExploreRequests(
        input,
      )) as RoleplayNarratorToolRequest[],
    autoCaptureRequest: async (input) =>
      (await bridge.roleplayNarratorAutoCaptureRequest(input)) as
        | RoleplayNarratorToolRequest
        | undefined,
    startTurn: async (input) =>
      (await bridge.startRoleplayNarratorTurn(
        input,
      )) as RoleplayNarratorPhasePlan,
    nextPhase: async (input) =>
      (await bridge.nextRoleplayNarratorPhase(
        input,
      )) as RoleplayNarratorPhasePlan,
    reviewRequestsRevision: async (feedback) =>
      bridge.roleplayNarratorReviewRequestsRevision(feedback),
  };
}
