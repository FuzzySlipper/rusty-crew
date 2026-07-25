import type {
  AgentMessage,
  AgentId,
  AdapterId,
  ChannelBindingRecord,
  EventReceipt,
  ExternalEvent,
  McpBindingRecord,
  McpSurfaceDiagnostics,
  McpSurfaceStatus,
  McpTransportKind,
  NormalizedChannelInboundMessage,
  NormalizedChannelOutboundMessage,
  NormalizedChannelActivityProjection,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export interface DenSuccessorGatewayEnv {
  DEN_SUCCESSOR_GATEWAY_URL?: string;
  DEN_SUCCESSOR_GATEWAY_API_PREFIX?: string;
  DEN_SUCCESSOR_DELIVERY_TOKEN?: string;
  DEN_SUCCESSOR_RUNTIME_TOKEN?: string;
  DEN_SUCCESSOR_OBSERVATION_WRITE_TOKEN?: string;
  DEN_SUCCESSOR_OBSERVATION_READ_TOKEN?: string;
  DEN_SUCCESSOR_CONVERSATION_WRITE_TOKEN?: string;
  DEN_SUCCESSOR_CONVERSATION_READ_TOKEN?: string;
  DEN_SUCCESSOR_TIMELINE_READ_TOKEN?: string;
  DEN_GATEWAY_SERVICE_TOKEN?: string;
  DEN_GATEWAY_RUNTIME_CALLER_TOKEN?: string;
  DEN_GATEWAY_API_PREFIX?: string;
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
  apiPrefix?: string;
  tokens: DenSuccessorGatewayTokens;
  timeoutMs?: number;
}

export interface DenSuccessorAgentIdentity {
  profile: string;
  instance_id: string;
  session_key?: string;
}

export interface DenSuccessorGatewayHealth {
  status: string;
  service_name?: string;
  version?: string;
  commit?: string;
  built_at?: string;
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

export interface DenConversationChannelResolution {
  channelId: number;
  projectId: string;
  slug: string;
}

export interface DenConversationChannelResolutionResult {
  resolutionsByBindingId: ReadonlyMap<string, DenConversationChannelResolution>;
  channelIdsByExternalId: ReadonlyMap<string, number>;
  membershipsByBindingId: ReadonlyMap<
    string,
    DenSuccessorConversationMembership
  >;
  membershipResolutionFailure?: string;
  createdCount: number;
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

export interface DenSuccessorGatewayClient {
  health(): Promise<DenSuccessorGatewayHealth>;
  registerRuntimeInstance(request: {
    instance_id: string;
    profile_identity: string;
    host: string;
    pid?: number;
  }): Promise<unknown>;
  heartbeatRuntimeInstance(instanceId: string): Promise<unknown>;
  createObservationActivityEvent(request: {
    source_domain: string;
    event_type: string;
    agent_identity?: DenSuccessorAgentIdentity;
    runtime_instance_id?: string;
    payload: Record<string, unknown>;
  }): Promise<unknown>;
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
  createConversationChannel(input: {
    slug: string;
    display_name: string;
    kind: string;
    project_id?: string;
    space_id?: string;
    created_by: string;
    visibility: string;
    settings?: Record<string, unknown>;
  }): Promise<DenSuccessorConversationChannel>;
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
  }): Promise<Array<{ id: number; body?: string; channel_id?: number }>>;
}

export type DenMemoryApiMode = "v1" | "den-memories-v0";

export interface DenMemoryClientPaths {
  read: string;
  search: string;
  recall: string;
  store: string;
  propose: string;
}

export interface DenMemorySourceRef {
  kind: string;
  ref: string;
  label?: string;
}

export interface DenMemoryRuntimeContext {
  projectId?: string;
  taskId?: string | number;
  sessionId?: string;
  agentId?: string;
  profileId?: string;
  runId?: string;
}

export interface DenMemoryScope {
  audience?: readonly string[];
  role?: string;
  mode?: "personal" | "project" | "shared" | string;
}

export interface DenMemoryRecord {
  id: string;
  slug?: string;
  title?: string;
  summary?: string;
  bodyMarkdown?: string;
  score?: number;
  audience?: readonly string[];
  role?: string;
  mode?: string;
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
  createdAt?: string;
  updatedAt?: string;
}

export interface DenMemoryListResponse {
  memories: DenMemoryRecord[];
  total?: number;
  nextCursor?: string;
}

export interface DenMemoryMutationResponse {
  accepted: boolean;
  memory?: DenMemoryRecord;
  proposalId?: string;
  reasonCode?: string;
  message?: string;
}

export interface DenMemoryClient {
  read(request: {
    id?: string;
    slug?: string;
    context?: DenMemoryRuntimeContext;
  }): Promise<DenMemoryRecord>;
  search(
    request: DenMemoryScope & {
      query: string;
      limit?: number;
      context?: DenMemoryRuntimeContext;
      sourceRefs?: readonly DenMemorySourceRef[];
      metadata?: Record<string, unknown>;
    },
  ): Promise<DenMemoryListResponse>;
  recall(
    request: DenMemoryScope & {
      prompt: string;
      limit?: number;
      context?: DenMemoryRuntimeContext;
      sourceRefs?: readonly DenMemorySourceRef[];
      metadata?: Record<string, unknown>;
    },
  ): Promise<DenMemoryListResponse>;
  store(
    request: DenMemoryScope & {
      title?: string;
      summary?: string;
      bodyMarkdown: string;
      context?: DenMemoryRuntimeContext;
      sourceRefs?: readonly DenMemorySourceRef[];
      metadata?: Record<string, unknown>;
    },
  ): Promise<DenMemoryMutationResponse>;
  propose(
    request: DenMemoryScope & {
      proposalKind?: "store" | "update" | "delete" | string;
      targetMemoryId?: string;
      title?: string;
      summary?: string;
      bodyMarkdown: string;
      context?: DenMemoryRuntimeContext;
      sourceRefs?: readonly DenMemorySourceRef[];
      metadata?: Record<string, unknown>;
    },
  ): Promise<DenMemoryMutationResponse>;
}

export interface DenMemoryClientErrorLike {
  code?: string;
  message?: string;
  options?: {
    status?: number;
    reasonCode?: string;
    retryable?: boolean;
  };
}

export type DenAdapterConnectionState =
  | "connected"
  | "degraded"
  | "disconnected";

export interface DenAdapterStatus {
  state: DenAdapterConnectionState;
  projectedEvents: number;
  droppedProjections: number;
  lastAcceptedSequence?: number;
  lastProjectionError?: string;
}

export interface ChannelBindingDiagnostics {
  bindingId: string;
  adapterId?: AdapterId;
  conversationProjectId?: string;
  conversationChannelId?: number;
  membershipStatus: string;
  presenceStatus: string;
  subscriptionStatus: string;
  degradedReason?: string;
  stale: boolean;
  lastObservedAt?: string;
  lastError?: string;
}

export interface McpToolDiscoveryClient {
  listTools(): Promise<McpDiscoveredTool[]> | McpDiscoveredTool[];
}

export interface McpToolExecutor {
  callTool(input: {
    binding: McpBindingRecord;
    toolName: string;
    arguments: unknown;
    toolCallId: string;
    signal?: AbortSignal;
  }): Promise<McpToolExecutionResult> | McpToolExecutionResult;
}

export interface McpToolExecutionResult {
  content:
    | string
    | Array<
        | { type: "text"; text: string }
        | { type: "image"; data: string; mimeType: string }
      >;
  details?: unknown;
  isError?: boolean;
}

export type JsonSchemaValue =
  | boolean
  | {
      type?: string | string[];
      title?: string;
      description?: string;
      properties?: Record<string, JsonSchemaValue>;
      required?: string[];
      items?: JsonSchemaValue;
      additionalProperties?: boolean | JsonSchemaValue;
      enum?: unknown[];
      const?: unknown;
      default?: unknown;
      minimum?: number;
      maximum?: number;
      minLength?: number;
      maxLength?: number;
      pattern?: string;
      anyOf?: JsonSchemaValue[];
      oneOf?: JsonSchemaValue[];
      allOf?: JsonSchemaValue[];
      $defs?: Record<string, JsonSchemaValue>;
      definitions?: Record<string, JsonSchemaValue>;
      [key: string]: unknown;
    };

export interface McpDiscoveredTool {
  name: string;
  description?: string;
  title?: string;
  inputSchema?: JsonSchemaValue;
  outputSchema?: JsonSchemaValue;
  annotations?: Record<string, unknown>;
}

export interface McpToolSourceIdentity {
  bindingId: string;
  adapterId: string;
  serverNames: readonly string[];
  sourceToolName: string;
  catalogRevision?: string;
  endpointRef: string;
}

export interface McpRegistryCandidate {
  name: string;
  description: string;
  category: "mcp";
  toolsets: readonly string[];
  implementationModule: string;
  surfaces: readonly ["brain", "mcp"];
  safety: readonly ("network_access" | "external_write")[];
  outputShape: string;
  version: string;
  inventoryTest: string;
  coexistenceNote?: string;
  source: McpToolSourceIdentity;
  parameters: unknown;
  outputSchema?: JsonSchemaValue;
  annotations: Record<string, unknown>;
}

export interface McpDiscoveryIssue {
  severity: "warning" | "error";
  code:
    | "invalid_name"
    | "schema_wrapped"
    | "schema_sanitized"
    | "duplicate_source_tool";
  toolName?: string;
  message: string;
}

export interface McpDiscoveryReport {
  bindingId: string;
  toolProfileKey: string;
  discoveredToolRevision?: string;
  candidates: McpRegistryCandidate[];
  issues: McpDiscoveryIssue[];
}

export interface McpTransportOpenRequest {
  binding: McpBindingRecord;
  endpointRef: string;
  serverNames: readonly string[];
}

export interface McpTransportClient {
  readonly kind: McpTransportKind | string;
  readonly name: string;
  connect(request: McpTransportOpenRequest): Promise<void> | void;
  disconnect(): Promise<void> | void;
  ping?(): Promise<void> | void;
}

export interface McpTransportFactory {
  readonly kind: McpTransportKind | string;
  create(binding: McpBindingRecord): McpTransportClient;
}

export interface McpConnectResult {
  bindingId: string;
  status: McpSurfaceStatus;
  transport: McpTransportKind | string;
  attemptCount: number;
  optional: boolean;
  degradedReason?: string;
}

export interface McpSurfaceManagerPort {
  connect(binding: McpBindingRecord): Promise<McpConnectResult>;
  reload(binding: McpBindingRecord): Promise<McpConnectResult>;
  reconnect(bindingId: string): Promise<McpConnectResult | undefined>;
  disconnect(bindingId: string): Promise<McpSurfaceDiagnostics | undefined>;
  archive(bindingId: string): Promise<McpSurfaceDiagnostics | undefined>;
  shutdown(): Promise<McpSurfaceDiagnostics[]>;
  diagnostics(bindingId: string): McpSurfaceDiagnostics | undefined;
  diagnostics(bindingId?: string): McpSurfaceDiagnostics[];
}

export interface TelegramBotApiClient {
  getUpdates?(request?: unknown): Promise<unknown[]> | unknown[];
  sendMessage(request: unknown): Promise<unknown> | unknown;
}

export interface TelegramUpdateOffsetStore {
  read(): Promise<number | undefined>;
  write(offset: number): Promise<void>;
}

export interface TelegramChannelConnectorPort {
  start(): Promise<void>;
  stop(): Promise<void> | void;
  pollOnce(): Promise<unknown>;
  sendOutbound(message: NormalizedChannelOutboundMessage): Promise<unknown>;
  diagnostics(): {
    bindingCount: number;
    lastPollAt?: string;
    lastError?: string;
  };
}

export interface TelegramConnectorFactoryInput {
  adapterId: AdapterId;
  botToken: string;
  apiBaseUrl?: string;
  pollTimeoutSeconds: number;
  pollIntervalMs: number;
  updateLimit: number;
  ttlMs: number;
  offsetStorePath: string;
  bindings(): readonly ChannelBindingRecord[];
  now(): string;
  onInbound(message: NormalizedChannelInboundMessage): Promise<void>;
}

export type ChannelIngressResult =
  | {
      status: "routed";
      message: NormalizedChannelInboundMessage;
    }
  | {
      status:
        | "expired"
        | "duplicate"
        | "stale_cursor"
        | "no_binding"
        | "ambiguous"
        | "inactive_binding"
        | "denied";
      reason: string;
      reasonCode?: string;
      correlationId?: string;
      message: NormalizedChannelInboundMessage;
      candidates?: ChannelBindingRecord[];
    };

export type ChannelRouteResolution =
  | {
      status: "routed";
      route: {
        from: AgentId;
        to: AgentId;
        body: string;
        correlationId: string;
        bindingId: string;
        sessionId?: string;
      };
      binding: ChannelBindingRecord;
    }
  | {
      status:
        | "no_binding"
        | "ambiguous"
        | "inactive_binding"
        | "expired"
        | "duplicate"
        | "denied";
      reason: string;
      reasonCode?: string;
      correlationId?: string;
      candidates: ChannelBindingRecord[];
      message: NormalizedChannelInboundMessage;
    };

export interface ChannelIngressRoutePlannerInput {
  message: NormalizedChannelInboundMessage;
  bindings: readonly ChannelBindingRecord[];
  routing?: {
    systemAgentId?: AgentId;
    mentionAliases?: Record<string, AgentId>;
  };
  now?: string;
}

export type ChannelOutboundProjectionResult =
  | {
      status: "projected";
      message: NormalizedChannelOutboundMessage;
      binding: ChannelBindingRecord;
    }
  | {
      status: "not_channel_target" | "no_binding" | "inactive_binding";
      reason: string;
      candidates: ChannelBindingRecord[];
    };

export type ChannelProjectionDispatchResult =
  | { accepted: true; kind: "message" | "activity" }
  | {
      accepted: false;
      kind: "message" | "activity";
      degradedReason: string;
    };

export interface ChannelIngressOptions {
  bridge: {
    injectExternalEvent(
      event: ExternalEvent,
    ): Promise<EventReceipt> | EventReceipt;
    routeAgentMessage(
      message: AgentMessage,
    ): Promise<EventReceipt> | EventReceipt;
  };
  bindings: readonly ChannelBindingRecord[];
  ensureSessionForRoute?(input: {
    message: NormalizedChannelInboundMessage;
    binding: ChannelBindingRecord;
  }): Promise<unknown> | unknown;
  now?: string;
  routing?: {
    systemAgentId?: AgentId;
    mentionAliases?: Record<string, AgentId>;
  };
  routePlanner?(
    input: ChannelIngressRoutePlannerInput,
  ): Promise<ChannelRouteResolution> | ChannelRouteResolution;
}

export interface ChannelProjectionSink {
  sendMessage(message: NormalizedChannelOutboundMessage): Promise<void> | void;
  sendActivity(
    activity: NormalizedChannelActivityProjection,
  ): Promise<void> | void;
}

export interface ServiceAdapterFactories {
  createDenSuccessorGatewayClient(
    config: DenSuccessorGatewayConfig,
  ): DenSuccessorGatewayClient;
  resolveDenConversationChannels(input: {
    client: Pick<
      DenSuccessorGatewayClient,
      | "listConversationChannels"
      | "createConversationChannel"
      | "listConversationMemberships"
    >;
    bindings: readonly ChannelBindingRecord[];
    defaultProjectId: string;
  }): Promise<DenConversationChannelResolutionResult>;
  createDenMemoryClient(input: {
    baseUrl: string;
    bearerToken?: string;
    timeoutMs?: number;
    paths?: Partial<DenMemoryClientPaths>;
    apiMode?: DenMemoryApiMode;
  }): DenMemoryClient;
  createMcpSurfaceManager(input: {
    transports: readonly McpTransportFactory[];
  }): McpSurfaceManagerPort;
  createSimulatedMcpTransportFactory(
    kind: McpTransportKind | string,
  ): McpTransportFactory;
  createTelegramAdapterRegistration(
    adapterId: AdapterId,
  ): PlatformAdapterRegistration;
  createTelegramConnector(
    input: TelegramConnectorFactoryInput,
  ): TelegramChannelConnectorPort;
  ingestChannelInboundMessage(
    message: NormalizedChannelInboundMessage,
    options: ChannelIngressOptions,
  ): Promise<ChannelIngressResult>;
  projectAgentMessageToChannel(
    message: AgentMessage,
    bindings: readonly ChannelBindingRecord[],
    options?: { maxBodyChars?: number; maxSummaryChars?: number; now?: string },
  ): ChannelOutboundProjectionResult;
  dispatchChannelMessageProjection(
    sink: ChannelProjectionSink,
    message: NormalizedChannelOutboundMessage,
  ): Promise<ChannelProjectionDispatchResult>;
}
