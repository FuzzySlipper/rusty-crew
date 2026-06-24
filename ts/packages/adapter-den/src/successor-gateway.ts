import type {
  ChannelBindingRecord,
  NormalizedChannelOutboundMessage,
} from "@rusty-crew/contracts";

const DEFAULT_GATEWAY_URL = "http://192.168.1.10:8079";
const MIGRATED_FUNCTIONS_HEADER = "X-Den-Migrated-Functions";

export interface DenSuccessorGatewayEnv {
  DEN_SUCCESSOR_GATEWAY_URL?: string;
  DEN_SUCCESSOR_DELIVERY_TOKEN?: string;
  DEN_SUCCESSOR_RUNTIME_TOKEN?: string;
  DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN?: string;
  DEN_SUCCESSOR_OBSERVATION_READ_TOKEN?: string;
  DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN?: string;
  DEN_SUCCESSOR_CONVERSATION_READ_TOKEN?: string;
  DEN_SUCCESSOR_TIMELINE_READ_TOKEN?: string;
  DEN_GATEWAY_SERVICE_TOKEN?: string;
  DEN_GATEWAY_RUNTIME_CALLER_TOKEN?: string;
  DEN_GATEWAY_OBSERVATION_WRITE_TOKEN?: string;
  DEN_GATEWAY_OBSERVATION_READ_TOKEN?: string;
  DEN_GATEWAY_CONVERSATION_WRITE_TOKEN?: string;
  DEN_GATEWAY_CONVERSATION_READ_TOKEN?: string;
  DEN_GATEWAY_TIMELINE_READ_TOKEN?: string;
}

export interface DenSuccessorGatewayTokens {
  delivery?: string;
  runtime?: string;
  observationWrite?: string;
  observationRead?: string;
  conversationWrite?: string;
  conversationRead?: string;
  timelineRead?: string;
}

export interface DenSuccessorGatewayConfig {
  gatewayUrl: string;
  tokens: DenSuccessorGatewayTokens;
  timeoutMs?: number;
}

export interface DenSuccessorAgentIdentity {
  profile: string;
  instance_id: string;
  session_key?: string;
}

export interface DenSuccessorActivityEventRequest {
  source_domain: string;
  event_type: string;
  agent_identity?: DenSuccessorAgentIdentity;
  runtime_instance_id?: string;
  payload: Record<string, unknown>;
}

export interface DenSuccessorCreateIntentRequest {
  target_identity: DenSuccessorAgentIdentity;
  idempotency_key: string;
  ttl_seconds?: number;
  source_ref?: string;
  channel_message_id?: number;
}

export interface DenSuccessorDeliveryIntent {
  id: number;
  target_identity: DenSuccessorAgentIdentity;
  state: string;
  idempotency_key: string;
  created_at: string;
  expires_at: string;
  source_ref?: string;
  channel_message_id?: number;
}

export interface DenSuccessorAppendMessageRequest {
  sender_type: string;
  sender_identity: string;
  body: string;
  message_kind: string;
  source_kind: string;
  source_id?: string;
  source_project_id?: string;
  target_project_id?: string;
  target_task_id?: number;
  profile_identity?: string;
  agent_instance_id?: string;
  session_id?: string;
  summary?: string;
  metadata?: Record<string, unknown>;
  dedupe_key?: string;
}

export interface DenSuccessorConversationChannel {
  id: number;
  slug: string;
  display_name: string;
  kind: string;
  project_id?: string;
  space_id?: string;
  created_by: string;
  visibility: string;
  settings?: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  archived_at?: string;
}

export interface DenSuccessorConversationMembership {
  id: number;
  channel_id: number;
  member_type: string;
  member_identity: string;
  profile_identity?: string;
  membership_status: string;
  wake_policy: string;
  can_send: boolean;
  can_react: boolean;
  can_invite: boolean;
  membership_purpose: string;
  settings?: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  left_at?: string;
}

export interface DenSuccessorCreateChannelRequest {
  slug: string;
  display_name: string;
  kind: string;
  project_id?: string;
  space_id?: string;
  created_by: string;
  visibility: string;
  settings?: Record<string, unknown>;
}

export interface DenSuccessorConversationMessage {
  id: number;
  channel_id: number;
  sender_type: string;
  sender_identity: string;
  body: string;
  message_kind: string;
  source_kind: string;
  source_id?: string;
  source_project_id?: string;
  target_project_id?: string;
  target_task_id?: number;
  assignment_id?: string;
  worker_run_id?: string;
  worker_role?: string;
  profile_identity?: string;
  agent_instance_id?: string;
  session_id?: string;
  summary?: string;
  deep_link?: string;
  thread_root_message_id?: number;
  reply_to_message_id?: number;
  metadata?: Record<string, unknown>;
  dedupe_key?: string;
  created_at: string;
}

export interface DenSuccessorGatewayHealth {
  status: string;
  service_name?: string;
  version?: string;
  commit?: string;
  built_at?: string;
}

export interface DenSuccessorRuntimeInstanceRequest {
  instance_id: string;
  profile_identity: string;
  host: string;
  pid?: number;
}

export interface DenSuccessorRuntimeInstance {
  instance_id: string;
  profile_identity: string;
  host: string;
  pid?: number;
  state: string;
  started_at: string;
  last_heartbeat_at?: string;
  stopped_at?: string;
  degraded_reason?: string;
}

export interface DenSuccessorGatewayClient {
  health(): Promise<DenSuccessorGatewayHealth>;
  registerRuntimeInstance(
    request: DenSuccessorRuntimeInstanceRequest,
  ): Promise<DenSuccessorRuntimeInstance>;
  heartbeatRuntimeInstance(
    instanceId: string,
  ): Promise<DenSuccessorRuntimeInstance>;
  getRuntimeInstance(instanceId: string): Promise<DenSuccessorRuntimeInstance>;
  createObservationActivityEvent(
    request: DenSuccessorActivityEventRequest,
  ): Promise<unknown>;
  createDeliveryIntent(
    request: DenSuccessorCreateIntentRequest,
  ): Promise<DenSuccessorDeliveryIntent>;
  listDeliveryIntents(
    state?: "pending" | "claimed" | "running" | "completed" | "failed",
  ): Promise<DenSuccessorDeliveryIntent[]>;
  claimDeliveryIntent(input: {
    id: number;
    claimToken: string;
    claimedBy: DenSuccessorAgentIdentity;
  }): Promise<DenSuccessorDeliveryIntent>;
  reportDeliveryIntentEvent(input: {
    id: number;
    claimToken: string;
    eventType: "running" | "completed" | "failed";
    payload?: Record<string, unknown>;
  }): Promise<DenSuccessorDeliveryIntent>;
  appendConversationMessage(input: {
    channelId: string | number;
    idempotencyKey: string;
    message: DenSuccessorAppendMessageRequest;
  }): Promise<unknown>;
  listConversationChannels(input?: {
    projectId?: string;
    kind?: string;
    limit?: number;
  }): Promise<DenSuccessorConversationChannel[]>;
  createConversationChannel(
    input: DenSuccessorCreateChannelRequest,
  ): Promise<DenSuccessorConversationChannel>;
  listConversationMemberships(input?: {
    channelId?: string | number;
    memberIdentity?: string;
    membershipPurpose?: string;
    projectId?: string;
    includeLeft?: boolean;
    limit?: number;
  }): Promise<DenSuccessorConversationMembership[]>;
  listConversationMessages(input: {
    channelId: string | number;
    limit?: number;
    afterId?: number;
  }): Promise<DenSuccessorConversationMessage[]>;
}

export function loadDenSuccessorGatewayConfig(
  env: DenSuccessorGatewayEnv = process.env,
): DenSuccessorGatewayConfig | undefined {
  const tokens: DenSuccessorGatewayTokens = {
    delivery:
      optionalEnv(env.DEN_SUCCESSOR_DELIVERY_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_SERVICE_TOKEN),
    runtime:
      optionalEnv(env.DEN_SUCCESSOR_RUNTIME_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_RUNTIME_CALLER_TOKEN),
    observationWrite:
      optionalEnv(env.DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_OBSERVATION_WRITE_TOKEN),
    observationRead:
      optionalEnv(env.DEN_SUCCESSOR_OBSERVATION_READ_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_OBSERVATION_READ_TOKEN),
    conversationWrite:
      optionalEnv(env.DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_CONVERSATION_WRITE_TOKEN),
    conversationRead:
      optionalEnv(env.DEN_SUCCESSOR_CONVERSATION_READ_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_CONVERSATION_READ_TOKEN),
    timelineRead:
      optionalEnv(env.DEN_SUCCESSOR_TIMELINE_READ_TOKEN) ??
      optionalEnv(env.DEN_GATEWAY_TIMELINE_READ_TOKEN),
  };
  const gatewayUrl =
    optionalEnv(env.DEN_SUCCESSOR_GATEWAY_URL) ?? DEFAULT_GATEWAY_URL;
  if (!Object.values(tokens).some((token) => token !== undefined)) {
    return undefined;
  }
  return { gatewayUrl, tokens };
}

export function createDenSuccessorGatewayClient(
  config: DenSuccessorGatewayConfig,
): DenSuccessorGatewayClient {
  const baseUrl = config.gatewayUrl.replace(/\/+$/, "");
  const timeoutMs = config.timeoutMs ?? 10_000;

  return {
    health() {
      return requestJSON<DenSuccessorGatewayHealth>({
        baseUrl,
        path: "/health",
        method: "GET",
        timeoutMs,
      });
    },
    createObservationActivityEvent(request) {
      return requestJSON({
        baseUrl,
        path: "/v1/observation/activity-events",
        method: "POST",
        token: requireToken(
          config.tokens.observationWrite,
          "observation write",
        ),
        body: request,
        timeoutMs,
      });
    },
    registerRuntimeInstance(request) {
      return requestJSON<DenSuccessorRuntimeInstance>({
        baseUrl,
        path: "/v1/runtime/instances",
        method: "POST",
        token: requireToken(config.tokens.runtime, "runtime"),
        body: request,
        timeoutMs,
      });
    },
    heartbeatRuntimeInstance(instanceId) {
      return requestJSON<DenSuccessorRuntimeInstance>({
        baseUrl,
        path: `/v1/runtime/instances/${encodeURIComponent(instanceId)}/heartbeat`,
        method: "POST",
        token: requireToken(config.tokens.runtime, "runtime"),
        body: {},
        timeoutMs,
      });
    },
    getRuntimeInstance(instanceId) {
      return requestJSON<DenSuccessorRuntimeInstance>({
        baseUrl,
        path: `/v1/runtime/instances/${encodeURIComponent(instanceId)}`,
        method: "GET",
        token: requireToken(config.tokens.runtime, "runtime"),
        timeoutMs,
      });
    },
    createDeliveryIntent(request) {
      return requestJSON<DenSuccessorDeliveryIntent>({
        baseUrl,
        path: "/v1/delivery/intents",
        method: "POST",
        token: requireToken(config.tokens.delivery, "delivery"),
        body: request,
        timeoutMs,
      });
    },
    listDeliveryIntents(state) {
      const query = state ? `?state=${encodeURIComponent(state)}` : "";
      return requestJSON<DenSuccessorDeliveryIntent[]>({
        baseUrl,
        path: `/v1/delivery/intents${query}`,
        method: "GET",
        token: requireToken(config.tokens.delivery, "delivery"),
        timeoutMs,
      });
    },
    claimDeliveryIntent(input) {
      return requestJSON<DenSuccessorDeliveryIntent>({
        baseUrl,
        path: `/v1/delivery/intents/${encodeURIComponent(String(input.id))}/claim`,
        method: "POST",
        token: requireToken(config.tokens.delivery, "delivery"),
        body: {
          claim_token: input.claimToken,
          claimed_by: input.claimedBy,
        },
        timeoutMs,
      });
    },
    reportDeliveryIntentEvent(input) {
      return requestJSON<DenSuccessorDeliveryIntent>({
        baseUrl,
        path: `/v1/delivery/intents/${encodeURIComponent(String(input.id))}/events`,
        method: "POST",
        token: requireToken(config.tokens.delivery, "delivery"),
        body: {
          claim_token: input.claimToken,
          event_type: input.eventType,
          payload: input.payload ?? {},
        },
        timeoutMs,
      });
    },
    appendConversationMessage(input) {
      return requestJSON({
        baseUrl,
        path: `/v1/conversation/channels/${encodeURIComponent(String(input.channelId))}/messages`,
        method: "POST",
        token: requireToken(
          config.tokens.conversationWrite,
          "conversation write",
        ),
        body: input.message,
        idempotencyKey: input.idempotencyKey,
        timeoutMs,
      });
    },
    listConversationChannels(input = {}) {
      const params = new URLSearchParams();
      if (input.projectId !== undefined)
        params.set("project_id", input.projectId);
      if (input.kind !== undefined) params.set("kind", input.kind);
      if (input.limit !== undefined) params.set("limit", String(input.limit));
      const query = params.size > 0 ? `?${params.toString()}` : "";
      return requestJSON<DenSuccessorConversationChannel[]>({
        baseUrl,
        path: `/v1/conversation/channels${query}`,
        method: "GET",
        token: requireToken(
          config.tokens.conversationRead,
          "conversation read",
        ),
        timeoutMs,
      });
    },
    createConversationChannel(input) {
      return requestJSON<DenSuccessorConversationChannel>({
        baseUrl,
        path: "/v1/conversation/channels",
        method: "POST",
        token: requireToken(
          config.tokens.conversationWrite,
          "conversation write",
        ),
        body: input,
        timeoutMs,
      });
    },
    listConversationMemberships(input = {}) {
      const params = new URLSearchParams();
      if (input.channelId !== undefined)
        params.set("channel_id", String(input.channelId));
      if (input.memberIdentity !== undefined)
        params.set("member_identity", input.memberIdentity);
      if (input.membershipPurpose !== undefined)
        params.set("membership_purpose", input.membershipPurpose);
      if (input.projectId !== undefined)
        params.set("project_id", input.projectId);
      if (input.includeLeft !== undefined)
        params.set("include_left", String(input.includeLeft));
      if (input.limit !== undefined) params.set("limit", String(input.limit));
      const query = params.size > 0 ? `?${params.toString()}` : "";
      return requestJSON<DenSuccessorConversationMembership[]>({
        baseUrl,
        path: `/v1/conversation/memberships${query}`,
        method: "GET",
        token: requireToken(
          config.tokens.conversationRead,
          "conversation read",
        ),
        timeoutMs,
      });
    },
    listConversationMessages(input) {
      const params = new URLSearchParams();
      if (input.limit !== undefined) params.set("limit", String(input.limit));
      if (input.afterId !== undefined)
        params.set("after_id", String(input.afterId));
      const query = params.size > 0 ? `?${params.toString()}` : "";
      return requestJSON<DenSuccessorConversationMessage[]>({
        baseUrl,
        path: `/v1/conversation/channels/${encodeURIComponent(String(input.channelId))}/messages${query}`,
        method: "GET",
        token: requireToken(
          config.tokens.conversationRead,
          "conversation read",
        ),
        timeoutMs,
      });
    },
  };
}

export function successorMessageFromProjection(
  message: NormalizedChannelOutboundMessage,
): DenSuccessorAppendMessageRequest {
  return {
    sender_type: "agent",
    sender_identity: message.runtime.agentId ?? "rusty-crew",
    body: message.body,
    message_kind: "message",
    source_kind: "rusty-crew",
    source_id: message.correlationId,
    profile_identity: message.runtime.profileId,
    agent_instance_id: message.runtime.instanceId,
    session_id: message.runtime.sessionId,
    metadata: {
      adapter_id: message.adapterId,
      binding_id: message.bindingId,
      provider: message.providerRefs.provider,
      correlation_id: message.correlationId,
      result_ref: message.resultRef,
      work_ref: message.workRef,
    },
    dedupe_key: message.idempotencyKey,
  };
}

export function successorChannelIdFromBinding(
  binding: ChannelBindingRecord,
): string {
  return binding.externalChannelId;
}

async function requestJSON<T = unknown>(input: {
  baseUrl: string;
  path: string;
  method: "GET" | "POST" | "PUT";
  token?: string;
  body?: unknown;
  idempotencyKey?: string;
  timeoutMs: number;
}): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), input.timeoutMs);
  try {
    const response = await fetch(input.baseUrl + input.path, {
      method: input.method,
      signal: controller.signal,
      headers: {
        Accept: "application/json",
        ...(input.body === undefined
          ? {}
          : { "Content-Type": "application/json" }),
        ...(input.token === undefined
          ? {}
          : { Authorization: `Bearer ${input.token}` }),
        ...(input.idempotencyKey === undefined
          ? {}
          : { "Idempotency-Key": input.idempotencyKey }),
        [MIGRATED_FUNCTIONS_HEADER]: "true",
      },
      body: input.body === undefined ? undefined : JSON.stringify(input.body),
    });
    const text = await response.text();
    if (!response.ok) {
      throw new Error(
        `Den successor Gateway ${input.method} ${input.path} failed: ${response.status} ${response.statusText}${text ? `: ${text.slice(0, 240)}` : ""}`,
      );
    }
    if (!text.trim()) return undefined as T;
    return JSON.parse(text) as T;
  } finally {
    clearTimeout(timer);
  }
}

function requireToken(token: string | undefined, tokenKind: string): string {
  if (token === undefined) {
    throw new Error(
      `Den successor Gateway ${tokenKind} token is not configured`,
    );
  }
  return token;
}

function optionalEnv(value: string | undefined): string | undefined {
  const trimmed = value?.trim();
  return trimmed ? trimmed : undefined;
}
