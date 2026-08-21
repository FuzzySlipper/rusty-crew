import type { ManifestOperationName } from "@rusty-crew/contracts";
import type { TSchema } from "typebox";

import {
  bridgeValidationEnabled,
  validateBridgeJsonText,
  validateBridgeValue,
  type BridgeValidationEnv,
} from "./bridge-validation.js";
import {
  nativeEventReceiptSchema,
  nativeHandleSchema,
  nativeQueuedMessageRecordSchema,
  nativeRuntimeBufferViewSchema,
  nativeSessionIdArraySchema,
  nativeSessionStateSummarySchema,
  nativeShutdownSummarySchema,
  rawBufferedBrainRunCleanupSummarySchema,
  rawBufferedBrainRunDiagnosticsSchema,
  rawDelegatedResourceCleanupReportSchema,
  rawDelegatedSessionRuntimeStatusSchema,
  rawGitHubGateTerminalReceiptSchema,
  rawModelProviderSecretSchema,
  rawNullableGitHubGateWaitRecordSchema,
  rawOpenAiOauthCodeExchangeResultSchema,
} from "./native-direct-validation-schemas.js";

interface DirectOutputDescriptor {
  operation: ManifestOperationName;
  schema: TSchema;
  encoding: "value" | "json_text";
  validateValue?: (output: unknown) => void;
}

const value = (
  operation: ManifestOperationName,
  schema: TSchema,
): DirectOutputDescriptor => ({ operation, schema, encoding: "value" });
const jsonText = (
  operation: ManifestOperationName,
  schema: TSchema,
): DirectOutputDescriptor => ({ operation, schema, encoding: "json_text" });
const runtimeBufferView = (): DirectOutputDescriptor => ({
  operation: "get_buffer",
  schema: nativeRuntimeBufferViewSchema,
  encoding: "value",
  validateValue(output) {
    const view = output as { byteLen?: unknown; bytes?: unknown };
    if (!(view.bytes instanceof Uint8Array)) {
      throw new TypeError("get_buffer bytes must be a Uint8Array");
    }
    if (view.byteLen !== view.bytes.byteLength) {
      throw new TypeError(
        `get_buffer byteLen ${String(view.byteLen)} does not match bytes length ${view.bytes.byteLength}`,
      );
    }
  },
});

const directOutputByMethod = {
  initializeEngine: value("initialize_engine", nativeHandleSchema),
  shutdownEngine: value("shutdown_engine", nativeShutdownSummarySchema),
  registerBrainImplementation: value(
    "register_brain_implementation",
    nativeHandleSchema,
  ),
  replaceBrainImplementation: value(
    "replace_brain_implementation",
    nativeHandleSchema,
  ),
  unregisterBrainImplementationForProfile: value(
    "unregister_brain_implementation_for_profile",
    nativeHandleSchema,
  ),
  registerPlatformAdapter: value(
    "register_platform_adapter",
    nativeHandleSchema,
  ),
  injectExternalEvent: value("inject_external_event", nativeEventReceiptSchema),
  injectDenDataUpdate: value(
    "inject_den_data_update",
    nativeEventReceiptSchema,
  ),
  enqueueBodyFollowUpMessage: value(
    "enqueue_body_follow_up_message",
    nativeQueuedMessageRecordSchema,
  ),
  archiveSession: value("archive_session", nativeSessionStateSummarySchema),
  ensureConfiguredSession: value(
    "ensure_configured_session",
    nativeSessionStateSummarySchema,
  ),
  setSessionReasoningEffort: value(
    "set_session_reasoning_effort",
    nativeSessionStateSummarySchema,
  ),
  cancelDelegatedSession: value(
    "cancel_delegated_session",
    nativeSessionStateSummarySchema,
  ),
  requestDelegatedCheckpoint: value(
    "request_delegated_checkpoint",
    nativeEventReceiptSchema,
  ),
  drainDelegatedSessions: value(
    "drain_delegated_sessions",
    nativeSessionIdArraySchema,
  ),
  cleanupDelegatedResourcesJson: jsonText(
    "cleanup_delegated_resources",
    rawDelegatedResourceCleanupReportSchema,
  ),
  delegatedSessionStatusJson: jsonText(
    "delegated_session_status",
    rawDelegatedSessionRuntimeStatusSchema,
  ),
  exchangeOpenaiOauthCodeJson: jsonText(
    "exchange_openai_oauth_code",
    rawOpenAiOauthCodeExchangeResultSchema,
  ),
  bufferedBrainRunDiagnosticsJson: jsonText(
    "buffered_brain_run_diagnostics",
    rawBufferedBrainRunDiagnosticsSchema,
  ),
  cleanupBufferedBrainRunsJson: jsonText(
    "cleanup_buffered_brain_runs",
    rawBufferedBrainRunCleanupSummarySchema,
  ),
  getModelProviderSecretJson: jsonText(
    "get_model_provider_secret",
    rawModelProviderSecretSchema,
  ),
  suspendForGithubGateJson: jsonText(
    "suspend_for_github_gate",
    rawNullableGitHubGateWaitRecordSchema,
  ),
  consumeGithubGateTerminalEventJson: jsonText(
    "consume_github_gate_terminal_event",
    rawGitHubGateTerminalReceiptSchema,
  ),
  recoverGithubGateWakes: value(
    "recover_github_gate_wakes",
    nativeHandleSchema,
  ),
  githubGateWaitJson: jsonText(
    "github_gate_wait",
    rawNullableGitHubGateWaitRecordSchema,
  ),
  githubGateEventCursor: value("github_gate_event_cursor", nativeHandleSchema),
  requeueLogicalTurnContinuations: value(
    "requeue_logical_turn_continuations",
    nativeHandleSchema,
  ),
  requeuePendingDirectAgentMessages: value(
    "requeue_pending_direct_agent_messages",
    nativeHandleSchema,
  ),
  subscribeEvents: value("subscribe_events", nativeHandleSchema),
  getBuffer: runtimeBufferView(),
} as const satisfies Record<string, DirectOutputDescriptor>;

export const directBridgeValidatedOperations = Object.freeze(
  Object.values(directOutputByMethod).map(({ operation }) => operation),
) as readonly ManifestOperationName[];

export function withDirectBridgeOutputValidation<T extends object>(
  binding: T,
  env: BridgeValidationEnv = process.env,
): T {
  if (!bridgeValidationEnabled(env)) return binding;

  return new Proxy(binding, {
    get(target, property, _receiver) {
      const member = Reflect.get(target, property, target);
      if (typeof property !== "string" || typeof member !== "function") {
        return member;
      }
      const descriptor = directOutputByMethod[
        property as keyof typeof directOutputByMethod
      ] as DirectOutputDescriptor | undefined;
      if (descriptor === undefined) return member.bind(target);

      return (...args: unknown[]) => {
        const result = Reflect.apply(member, target, args) as unknown;
        if (result instanceof Promise) {
          return result.then((resolved) => {
            validateOutput(descriptor, resolved, env);
            return resolved;
          });
        }
        validateOutput(descriptor, result, env);
        return result;
      };
    },
  });
}

function validateOutput(
  descriptor: DirectOutputDescriptor,
  output: unknown,
  env: BridgeValidationEnv,
): void {
  if (descriptor.encoding === "json_text") {
    if (typeof output !== "string") {
      throw new TypeError(
        `direct bridge output validation expected JSON text for ${descriptor.operation}`,
      );
    }
    validateBridgeJsonText({
      operation: descriptor.operation,
      direction: "rust_to_ts",
      schema: descriptor.schema,
      text: output,
      env,
    });
    return;
  }

  validateBridgeValue({
    operation: descriptor.operation,
    direction: "rust_to_ts",
    schema: descriptor.schema,
    value: output,
    env,
  });
  descriptor.validateValue?.(output);
}
