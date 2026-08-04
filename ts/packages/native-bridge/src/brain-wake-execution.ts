import type {
  BrainImplementationHandle,
  BrainImplementationRegistration,
  BrainWakeAccepted,
  BrainWakeRequest,
} from "@rusty-crew/contracts";

import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import {
  brainWakeAcceptedSchema,
  brainWakeRequestSchema,
} from "./bridge-validation-schemas.js";
import { validateBridgeValue } from "./bridge-validation.js";
import type { BrainWakeExecutor } from "./brain-wake-public-api.js";
import {
  type NativeBridgeModule,
  type NativeProviderStateDiagnostic,
} from "./public-api.js";
import { brainWakeStreamItemsFromExecutionResult } from "./brain-wake-stream.js";

class BrainActionAdmissionError extends Error {
  readonly reasonCode = "brain_action_rejected";

  constructor(message: string) {
    super(message);
    this.name = "BrainActionAdmissionError";
  }
}

export interface NativeBrainWakeExecutionContext {
  binding: NativeBridgeBinding;
  module: NativeBridgeModule;
  wakeExecutors: ReadonlyMap<BrainImplementationHandle, BrainWakeExecutor>;
  brainRegistrations: ReadonlyMap<
    BrainImplementationHandle,
    BrainImplementationRegistration
  >;
  observeProviderStateSaveFailure(
    request: Pick<BrainWakeRequest, "sessionId" | "wakeId">,
    registration: BrainImplementationRegistration | undefined,
    status: Extract<NativeProviderStateDiagnostic["status"], "save_failed">,
  ): void;
}

export async function executeNativeBrainWake(
  context: NativeBrainWakeExecutionContext,
  request: BrainWakeRequest,
  options?: { signal?: AbortSignal },
): Promise<BrainWakeAccepted> {
  const validatedRequest = validateBridgeValue<BrainWakeRequest>({
    operation: "wake_brain",
    direction: "ts_to_rust",
    schema: brainWakeRequestSchema,
    value: request,
  });
  const executor = context.wakeExecutors.get(validatedRequest.brain);
  if (!executor) {
    throw new Error(
      `brain implementation handle ${validatedRequest.brain} is not registered in the TS runtime`,
    );
  }

  try {
    const result = await executor.wake(
      validatedRequest,
      context.module,
      options,
    );
    for (const item of brainWakeStreamItemsFromExecutionResult(
      validatedRequest,
      result,
    )) {
      switch (item.type) {
        case "event":
          await context.module.submitBrainEvent(item.event);
          break;
        case "actions": {
          if (item.batch.actions.length === 0) break;
          const receipt = await context.module.submitBrainActions(item.batch);
          if (receipt.rejectedActions.length > 0) {
            const rejectionSummary = receipt.rejectedActions
              .map(
                (rejection) =>
                  `action ${rejection.index}: ${rejection.kind}: ${rejection.message}`,
              )
              .join("; ");
            await context.module
              .submitBrainEvent({
                wakeId: validatedRequest.wakeId,
                sessionId: validatedRequest.sessionId,
                event: {
                  type: "provider_status",
                  level: "error",
                  message: `Rust rejected ${receipt.rejectedActions.length} brain action(s): ${rejectionSummary}`,
                  metadataJson: JSON.stringify({
                    source: "brain_action_admission",
                    reason_code: "brain_action_rejected",
                    accepted_action_count: receipt.acceptedActions,
                    rejected_actions: receipt.rejectedActions,
                  }),
                },
              })
              .catch(() => {
                // The rejected receipt remains authoritative if diagnostics degrade.
              });
            throw new BrainActionAdmissionError(
              `wake ${validatedRequest.wakeId} brain action admission rejected: ${rejectionSummary}`,
            );
          }
          if (receipt.acceptedActions !== item.batch.actions.length) {
            throw new BrainActionAdmissionError(
              `wake ${validatedRequest.wakeId} brain action admission count mismatch: expected ${item.batch.actions.length}, accepted ${receipt.acceptedActions}`,
            );
          }
          break;
        }
        case "wake_failed":
          throw new Error(
            `brain wake ${item.failure.wakeId} failed: ${item.failure.message}`,
          );
      }
    }
    if (result.providerState !== undefined) {
      try {
        context.binding.applyBrainProviderStateOutputJson(
          validatedRequest.brain,
          validatedRequest.sessionId,
          validatedRequest.wakeId,
          JSON.stringify(result.providerState),
        );
      } catch (error) {
        context.observeProviderStateSaveFailure(
          validatedRequest,
          context.brainRegistrations.get(validatedRequest.brain),
          "save_failed",
        );
        await context.module.submitBrainEvent({
          wakeId: validatedRequest.wakeId,
          sessionId: validatedRequest.sessionId,
          event: {
            type: "provider_status",
            level: "degraded",
            message: `provider state save failed: ${errorMessage(error)}`,
          },
        });
      }
    }
    const settlement = JSON.parse(
      context.binding.settleBrainWakeJson(
        JSON.stringify({
          wake_id: validatedRequest.wakeId,
          outcome: result.outcome ?? "completed",
          ...(result.transportMetrics === undefined
            ? {}
            : {
                progress: {
                  providerRequestCount:
                    result.transportMetrics.providerRequestCount,
                  toolRoundCount:
                    "toolRoundCount" in result.transportMetrics
                      ? result.transportMetrics.toolRoundCount
                      : result.transportMetrics.continuationRoundCount,
                },
              }),
          ...(result.continuationState === undefined
            ? {}
            : { continuation_state: result.continuationState }),
          ...(result.attention === undefined
            ? {}
            : { attention: result.attention }),
        }),
      ),
    ) as { outcome: BrainWakeAccepted["outcome"] };
    return validateBridgeValue<BrainWakeAccepted>({
      operation: "wake_brain",
      direction: "rust_to_ts",
      schema: brainWakeAcceptedSchema,
      value: {
        wakeId: validatedRequest.wakeId,
        accepted: true,
        outcome: settlement.outcome,
      },
    });
  } catch (error) {
    try {
      const settlement = JSON.parse(
        context.binding.settleBrainWakeJson(
          JSON.stringify({
            wake_id: validatedRequest.wakeId,
            outcome: "failed",
            reason_code:
              error instanceof BrainActionAdmissionError
                ? error.reasonCode
                : "brain_wake_failed",
            summary: errorMessage(error),
          }),
        ),
      ) as { phase?: string };
      if (settlement.phase === "cancelled") {
        return validateBridgeValue<BrainWakeAccepted>({
          operation: "wake_brain",
          direction: "rust_to_ts",
          schema: brainWakeAcceptedSchema,
          value: {
            wakeId: validatedRequest.wakeId,
            accepted: true,
            outcome: "completed",
          },
        });
      }
    } catch (settlementError) {
      throw new Error(
        `${errorMessage(error)}; logical turn failure settlement also failed: ${errorMessage(settlementError)}`,
      );
    }
    throw error;
  }
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
