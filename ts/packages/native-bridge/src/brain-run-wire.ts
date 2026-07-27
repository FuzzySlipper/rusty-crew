import type {
  BrainAction,
  BrainWakeProviderStateOutput,
  BrainWakeProviderStateInput,
  BrainWakeFailure,
  BrainWakeStreamItem,
  CompletionPacket,
  ProfileId,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";

import type {
  BrainWakeExecutionResult,
  NativeBufferedBrainRunDrain,
  OpenAiResponsesTransportMetrics,
  NativeOpenAiOauthExchangeError,
  OpenAiResponsesBrainRunInput,
  ChatCompletionsBrainRunInput,
  ChatCompletionsTransportMetrics,
  NativeBrainWakeProviderStateInput,
  NativeModelProviderCredentialKind,
} from "./public-api.js";

import {
  toNativeBodyState,
  toNativeBrainEventForJson,
  toAgentMessage,
  toBrainEvent,
  type RawAgentMessage,
  type RawBrainEvent,
} from "./event-body-wire.js";
import type { RawResourceLimits } from "./runtime-config-wire.js";
import * as streamRetention from "./brain-stream-retention-wire.js";
import {
  chatCompletionsTransportMetricsFromRaw,
  type RawChatCompletionsTransportMetrics,
} from "./chat-completions-metrics-wire.js";
export { chatCompletionsTransportMetricsFromRaw };

export function assertCanonicalBrainRunModule(
  moduleId: string,
): "chat-completions" | "openai-responses" {
  if (moduleId === "chat-completions" || moduleId === "openai-responses") {
    return moduleId;
  }
  throw new Error(`native bridge returned unknown brain module ${moduleId}`);
}

export function toNativeBrainAction(action: BrainAction): unknown {
  switch (action.type) {
    case "send_message":
      return {
        type: action.type,
        message: {
          from: action.message.from,
          to: action.message.to,
          body: action.message.body,
          correlation_id: action.message.correlationId,
        },
      };
    case "request_delegation":
      return {
        type: action.type,
        profile_id: action.profileId,
        task_id: action.taskId,
        prompt: action.prompt,
        expected_output: action.expectedOutput,
        resource_limits: action.resourceLimits
          ? {
              workdir: action.resourceLimits.workdir,
              max_duration_ms: action.resourceLimits.maxDurationMs,
              max_delegation_depth: action.resourceLimits.maxDelegationDepth,
            }
          : undefined,
        timeout_ms: action.timeoutMs,
        priority: action.priority,
        fan_out_group_id: action.fanOutGroupId,
        fan_out_max_concurrency: action.fanOutMaxConcurrency,
        fan_out_failure_policy: action.fanOutFailurePolicy,
        correlation_id: action.correlationId,
        parent_consumption: action.parentConsumption,
        capacity_request: action.capacityRequest
          ? {
              member_id: action.capacityRequest.memberId,
              claim_ttl_ms: action.capacityRequest.claimTtlMs,
              fallback_policy: action.capacityRequest.fallbackPolicy,
            }
          : undefined,
      };
    case "deliver_completion":
      return {
        type: action.type,
        packet: {
          session_id: action.packet.sessionId,
          status: action.packet.status,
          summary: action.packet.summary,
        },
      };
  }
}

export function toNativeOpenAiResponsesBrainRunInput(
  input: OpenAiResponsesBrainRunInput,
): unknown {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    bodyState: toNativeBodyState(input.bodyState),
    tools: input.tools?.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
    providerState: input.providerState
      ? toNativeProviderStateInput(input.providerState)
      : undefined,
    providerStateAbsence: input.providerStateAbsence,
    config: input.config,
    client:
      input.client?.mode === "live"
        ? {
            mode: "live",
            base_url: input.client.baseUrl,
            api_key: input.client.apiKey,
            auth_kind: input.client.authKind,
            provider_alias: input.client.providerAlias,
            oauth_credential_secret: input.client.oauthCredentialSecret,
          }
        : { mode: "fake" },
  };
}

export function toNativeChatCompletionsBrainRunInput(
  input: ChatCompletionsBrainRunInput,
): unknown {
  return {
    wakeId: input.wakeId,
    sessionId: input.sessionId,
    messages: input.messages.map((message) => ({
      role: message.role,
      content: message.content,
      reasoning_content: message.reasoningContent,
      name: message.name,
      tool_call_id: message.toolCallId,
      tool_calls: message.toolCalls,
    })),
    inputImages: input.inputImages,
    providerState: input.providerState
      ? toNativeProviderStateInput(input.providerState)
      : undefined,
    tools: input.tools?.map((tool) => ({
      name: tool.name,
      description: tool.description,
      inputSchema: tool.inputSchema,
    })),
    config: input.config,
    client:
      input.client?.mode === "live"
        ? {
            mode: "live",
            base_url: input.client.baseUrl,
            api_key: input.client.apiKey,
          }
        : { mode: "fake" },
  };
}

export function toNativeProviderStateInput(
  state: BrainWakeProviderStateInput,
): NativeBrainWakeProviderStateInput {
  return {
    module_id: state.moduleId,
    strategy_id: state.strategyId,
    profile_fingerprint: state.profileFingerprint,
    provider_fingerprint: state.providerFingerprint,
    payload_version: state.payloadVersion,
    payload: state.payload,
    expires_at: state.expiresAt ?? undefined,
  };
}

export function toBrainWakeStreamItem(
  item: RawBrainWakeStreamItem,
): BrainWakeStreamItem {
  switch (item.type) {
    case "event":
      return {
        type: "event",
        event: {
          wakeId: item.event.wake_id,
          sessionId: item.event.session_id,
          event: toBrainEvent(item.event.event),
        },
      };
    case "actions":
      return {
        type: "actions",
        batch: {
          wakeId: item.batch.wake_id,
          sessionId: item.batch.session_id,
          actions: item.batch.actions.map(toBrainAction),
        },
      };
    case "wake_failed":
      return {
        type: "wake_failed",
        failure: {
          wakeId: item.failure.wake_id,
          sessionId: item.failure.session_id,
          kind: item.failure.kind as BrainWakeFailure["kind"],
          ...(item.failure.reason_code === undefined
            ? {}
            : { reasonCode: item.failure.reason_code }),
          message: item.failure.message,
        },
      };
  }
}

export function toOpenAiResponsesBrainRunResult(
  raw: RawOpenAiResponsesBrainRunResult,
): BrainWakeExecutionResult {
  return {
    stream: raw.stream.map(toBrainWakeStreamItem),
    events: [],
    actions: [],
    providerState: raw.provider_state
      ? toBrainWakeProviderStateOutput(raw.provider_state)
      : undefined,
    transportMetrics: raw.transport_metrics,
    credentialSecretUpdate: raw.credential_secret_update
      ? {
          providerAlias: raw.credential_secret_update.provider_alias,
          secret: raw.credential_secret_update.secret,
        }
      : undefined,
  };
}

export function toRawOpenAiResponsesBrainRunResult(
  result: BrainWakeExecutionResult & {
    transportMetrics?: OpenAiResponsesTransportMetrics;
  },
): RawOpenAiResponsesBrainRunResult {
  return {
    stream: (
      result.stream ?? [
        ...result.events.map(
          (event): BrainWakeStreamItem => ({
            type: "event",
            event,
          }),
        ),
        ...(result.actions.length > 0
          ? [
              {
                type: "actions" as const,
                batch: {
                  wakeId: result.events[0]?.wakeId ?? "unknown-wake",
                  sessionId: result.events[0]?.sessionId ?? "unknown-session",
                  actions: result.actions,
                },
              },
            ]
          : []),
      ]
    ).map(toRawBrainWakeStreamItem),
    provider_state: result.providerState
      ? toRawBrainWakeProviderStateOutput(result.providerState)
      : undefined,
    transport_metrics: result.transportMetrics,
    credential_secret_update: result.credentialSecretUpdate
      ? {
          provider_alias: result.credentialSecretUpdate.providerAlias,
          secret: result.credentialSecretUpdate.secret,
        }
      : undefined,
  };
}

export function toRawBrainWakeStreamItem(
  item: BrainWakeStreamItem,
): RawBrainWakeStreamItem {
  switch (item.type) {
    case "event":
      return {
        type: "event",
        event: {
          wake_id: item.event.wakeId,
          session_id: item.event.sessionId,
          event: toNativeBrainEventForJson(item.event.event) as RawBrainEvent,
        },
      };
    case "actions":
      return {
        type: "actions",
        batch: {
          wake_id: item.batch.wakeId,
          session_id: item.batch.sessionId,
          actions: item.batch.actions.map(
            (action) => toNativeBrainAction(action) as RawBrainAction,
          ),
        },
      };
    case "wake_failed":
      return {
        type: "wake_failed",
        failure: {
          wake_id: item.failure.wakeId,
          session_id: item.failure.sessionId,
          kind: item.failure.kind,
          ...(item.failure.reasonCode === undefined ||
          item.failure.reasonCode === null
            ? {}
            : { reason_code: item.failure.reasonCode }),
          message: item.failure.message,
        },
      };
  }
}

export function toBrainAction(action: RawBrainAction): BrainAction {
  switch (action.type) {
    case "send_message":
      return {
        type: action.type,
        message: toAgentMessage(action.message),
      };
    case "request_delegation":
      return {
        type: action.type,
        profileId: action.profile_id,
        taskId: action.task_id,
        prompt: action.prompt,
        expectedOutput: action.expected_output,
        resourceLimits: action.resource_limits
          ? {
              workdir: action.resource_limits.workdir,
              maxDurationMs: action.resource_limits.max_duration_ms,
              maxDelegationDepth: action.resource_limits.max_delegation_depth,
            }
          : undefined,
        timeoutMs: action.timeout_ms,
        priority: action.priority,
        fanOutGroupId: action.fan_out_group_id,
        fanOutMaxConcurrency: action.fan_out_max_concurrency,
        fanOutFailurePolicy: action.fan_out_failure_policy,
        correlationId: action.correlation_id,
        parentConsumption: action.parent_consumption,
        capacityRequest: action.capacity_request
          ? {
              memberId: action.capacity_request.member_id,
              claimTtlMs: action.capacity_request.claim_ttl_ms,
              fallbackPolicy: action.capacity_request.fallback_policy,
            }
          : undefined,
      };
    case "deliver_completion":
      return {
        type: action.type,
        packet: {
          sessionId: action.packet.session_id,
          status: action.packet.status,
          summary: action.packet.summary,
        },
      };
  }
}

export function toRawBrainWakeProviderStateOutput(
  output: BrainWakeProviderStateOutput,
): RawBrainWakeProviderStateOutput {
  switch (output.type) {
    case "unchanged":
      return { type: "unchanged" };
    case "replace":
      return {
        type: "replace",
        state: {
          module_id: output.state.moduleId,
          strategy_id: output.state.strategyId,
          profile_fingerprint: output.state.profileFingerprint,
          provider_fingerprint: output.state.providerFingerprint,
          payload_version: output.state.payloadVersion,
          payload: output.state.payload,
          ttl_ms: output.state.ttlMs ?? undefined,
        },
      };
    case "clear":
      return { type: "clear", reason: output.reason };
  }
}

export function toBrainWakeProviderStateOutput(
  output: RawBrainWakeProviderStateOutput,
): BrainWakeProviderStateOutput {
  switch (output.type) {
    case "unchanged":
      return { type: "unchanged" };
    case "replace":
      return {
        type: "replace",
        state: {
          moduleId: output.state.module_id,
          strategyId: output.state.strategy_id,
          profileFingerprint: output.state.profile_fingerprint,
          providerFingerprint: output.state.provider_fingerprint,
          payloadVersion: output.state.payload_version,
          payload: output.state.payload,
          ttlMs: output.state.ttl_ms,
        },
      };
    case "clear":
      return { type: "clear", reason: output.reason };
  }
}

export function toBufferedBrainRunDrainResult(
  raw: RawBufferedBrainRunDrainResult,
): NativeBufferedBrainRunDrain {
  const moduleId = assertCanonicalBrainRunModule(raw.module_id);
  const transportMetrics =
    raw.transport_metrics == null
      ? undefined
      : moduleId === "chat-completions"
        ? chatCompletionsTransportMetricsFromRaw(
            raw.transport_metrics as NonNullable<
              RawChatCompletionsBufferedDrainResult["transport_metrics"]
            >,
          )
        : (raw.transport_metrics as OpenAiResponsesTransportMetrics);
  return {
    moduleId,
    wakeId: raw.wake_id,
    items: raw.items.map(toBrainWakeStreamItem),
    toolRequests: raw.tool_requests.map((request) => ({
      wakeId: request.wake_id ?? raw.wake_id,
      callId: request.call_id,
      ...(request.provider_item_id == null
        ? {}
        : { providerItemId: request.provider_item_id }),
      name: request.name,
      argumentsJson: request.arguments_json,
    })),
    streamRetentionMetrics: streamRetention.decodeStreamRetention(
      raw.stream_retention_metrics,
    ),
    terminal: raw.terminal,
    ...(raw.terminal_reason_code == null
      ? {}
      : { terminalReasonCode: raw.terminal_reason_code }),
    ...(raw.provider_state == null
      ? {}
      : { providerState: toBrainWakeProviderStateOutput(raw.provider_state) }),
    ...(transportMetrics === undefined ? {} : { transportMetrics }),
    ...(raw.credential_secret_update == null
      ? {}
      : {
          credentialSecretUpdate: {
            providerAlias: raw.credential_secret_update.provider_alias,
            secret: raw.credential_secret_update.secret,
          },
        }),
    ...(raw.cancellation == null
      ? {}
      : {
          cancellation: {
            reasonCode: raw.cancellation.reason_code,
            summary: raw.cancellation.summary,
            cancelledAt: raw.cancellation.cancelled_at,
          },
        }),
    ...(raw.error == null ? {} : { error: raw.error }),
  };
}

export function toRawBufferedBrainRunDrainResult(
  result: NativeBufferedBrainRunDrain,
): RawBufferedBrainRunDrainResult {
  const chatMetrics =
    result.transportMetrics as ChatCompletionsTransportMetrics;
  const transportMetrics =
    result.transportMetrics === undefined
      ? undefined
      : result.moduleId === "chat-completions"
        ? {
            provider_request_count: chatMetrics.providerRequestCount,
            tool_round_count: chatMetrics.toolRoundCount,
            provider_event_counts: chatMetrics.providerEventCounts,
          }
        : (result.transportMetrics as OpenAiResponsesTransportMetrics);
  return {
    module_id: result.moduleId,
    wake_id: result.wakeId,
    items: result.items.map(toRawBrainWakeStreamItem),
    tool_requests: result.toolRequests.map((request) => ({
      wake_id: request.wakeId,
      call_id: request.callId,
      ...(request.providerItemId === undefined
        ? {}
        : { provider_item_id: request.providerItemId }),
      name: request.name,
      arguments_json: request.argumentsJson,
    })),
    stream_retention_metrics: streamRetention.encodeStreamRetention(
      result.streamRetentionMetrics,
    ),
    terminal: result.terminal,
    ...(result.terminalReasonCode === undefined
      ? {}
      : { terminal_reason_code: result.terminalReasonCode }),
    ...(result.providerState === undefined
      ? {}
      : {
          provider_state: toRawBrainWakeProviderStateOutput(
            result.providerState,
          ),
        }),
    ...(transportMetrics === undefined
      ? {}
      : { transport_metrics: transportMetrics }),
    ...(result.credentialSecretUpdate === undefined
      ? {}
      : {
          credential_secret_update: {
            provider_alias: result.credentialSecretUpdate.providerAlias,
            secret: result.credentialSecretUpdate.secret,
          },
        }),
    ...(result.cancellation === undefined
      ? {}
      : {
          cancellation: {
            reason_code: result.cancellation.reasonCode,
            summary: result.cancellation.summary,
            cancelled_at: result.cancellation.cancelledAt,
          },
        }),
    ...(result.error === undefined ? {} : { error: result.error }),
  };
}

export interface RawOpenAiResponsesBrainRunResult {
  stream: RawBrainWakeStreamItem[];
  provider_state?: RawBrainWakeProviderStateOutput | null;
  transport_metrics?: OpenAiResponsesTransportMetrics;
  credential_secret_update?: RawOpenAiResponsesCredentialSecretUpdate;
}

export interface RawOpenAiResponsesCredentialSecretUpdate {
  provider_alias: string;
  secret: string;
}

export interface RawOpenAiOauthCredentialSummary {
  kind: NativeModelProviderCredentialKind;
  version: number;
  has_secret: boolean;
  account_id?: string | null;
  email?: string | null;
  plan_type?: string | null;
  is_fedramp_account: boolean;
  access_token_expires_at?: string | null;
}

export type RawOpenAiOauthCodeExchangeResult =
  | {
      ok: true;
      secret: string;
      summary: RawOpenAiOauthCredentialSummary;
    }
  | {
      ok: false;
      error: NativeOpenAiOauthExchangeError;
    };

export interface RawOpenAiResponsesBufferedStartResult {
  wake_id: string;
}

export interface RawChatCompletionsBufferedStartResult {
  wake_id: string;
}

export interface RawOpenAiResponsesBufferedDrainResult {
  wake_id: string;
  items: RawBrainWakeStreamItem[];
  tool_requests?: Array<{
    call_id: string;
    provider_item_id?: string | null;
    name: string;
    arguments_json: string;
  }>;
  terminal: boolean;
  provider_state?: RawBrainWakeProviderStateOutput;
  transport_metrics?: OpenAiResponsesTransportMetrics;
  credential_secret_update?: RawOpenAiResponsesCredentialSecretUpdate;
  error?: string | null;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

export interface RawChatCompletionsBufferedDrainResult {
  wake_id: string;
  items: RawBrainWakeStreamItem[];
  tool_requests?: Array<{
    call_id: string;
    provider_item_id?: string | null;
    name: string;
    arguments_json: string;
  }>;
  terminal: boolean;
  provider_state?: RawBrainWakeProviderStateOutput | null;
  transport_metrics?: RawChatCompletionsTransportMetrics;
  error?: string | null;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

export interface RawBufferedBrainRunDrainResult {
  module_id: string;
  wake_id: string;
  items: RawBrainWakeStreamItem[];
  tool_requests: Array<{
    wake_id?: string;
    call_id: string;
    provider_item_id?: string | null;
    name: string;
    arguments_json: string;
  }>;
  stream_retention_metrics: streamRetention.RawBufferedBrainStreamRetentionMetrics;
  terminal: boolean;
  terminal_reason_code?: string | null;
  provider_state?: RawBrainWakeProviderStateOutput | null;
  transport_metrics?:
    | OpenAiResponsesTransportMetrics
    | NonNullable<RawChatCompletionsBufferedDrainResult["transport_metrics"]>
    | null;
  credential_secret_update?: RawOpenAiResponsesCredentialSecretUpdate | null;
  error?: string | null;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

export interface RawOpenAiResponsesBufferedCancellation {
  reason_code: string;
  summary: string;
  cancelled_at: string;
}

export interface RawOpenAiResponsesBufferedCancelResult {
  ok: true;
  wake_id: string;
  cancelled: boolean;
  terminal: boolean;
  cancellation?: RawOpenAiResponsesBufferedCancellation | null;
}

export type RawBrainWakeStreamItem =
  | {
      type: "event";
      event: {
        wake_id: string;
        session_id: SessionId;
        event: RawBrainEvent;
      };
    }
  | {
      type: "actions";
      batch: {
        wake_id: string;
        session_id: SessionId;
        actions: RawBrainAction[];
      };
    }
  | {
      type: "wake_failed";
      failure: {
        wake_id: string;
        session_id: SessionId;
        kind: string;
        reason_code?: string;
        message: string;
      };
    };

export type RawBrainAction =
  | {
      type: "send_message";
      message: RawAgentMessage;
    }
  | {
      type: "request_delegation";
      profile_id: ProfileId;
      task_id?: TaskId;
      prompt: string;
      expected_output?: string;
      resource_limits?: RawResourceLimits;
      timeout_ms?: number;
      priority?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["priority"];
      fan_out_group_id?: string;
      fan_out_max_concurrency?: number;
      fan_out_failure_policy?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["fanOutFailurePolicy"];
      correlation_id?: string;
      parent_consumption?: Extract<
        BrainAction,
        { type: "request_delegation" }
      >["parentConsumption"];
      capacity_request?: {
        member_id: string;
        claim_ttl_ms?: number;
        fallback_policy?: "reject_on_no_capacity" | "direct_on_no_capacity";
      };
    }
  | {
      type: "deliver_completion";
      packet: {
        session_id: SessionId;
        status: CompletionPacket["status"];
        summary: string;
      };
    };

export type RawBrainWakeProviderStateOutput =
  | { type: "unchanged" }
  | {
      type: "replace";
      state: NativeBrainWakeProviderStateInput & { ttl_ms?: number };
    }
  | { type: "clear"; reason: "brain_requested_clear" };
