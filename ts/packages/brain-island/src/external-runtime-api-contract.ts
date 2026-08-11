import type {
  DenRuntimeReference,
  ExternalAgentBindingLineage,
  ExternalAgentSessionCreationRecord,
  ExternalControlReceipt,
  ExternalRuntimeRegistration,
} from "@rusty-crew/contracts";

export const EXTERNAL_RUNTIME_API_CONTRACT_VERSION = "0.19.0";

export const EXTERNAL_THREAD_READ_API_REASON_CODES = [
  "external_thread_cursor_invalid",
  "external_thread_cursor_stale",
  "external_thread_cursor_out_of_range",
  "external_thread_listing_limit_exceeded",
] as const;

export const EXTERNAL_THREAD_LIFECYCLE_API_REASON_CODES = [
  "external_thread_not_found",
  "external_thread_active",
  "external_thread_interaction_pending",
  "external_thread_context_unavailable",
  "external_thread_listing_limit_exceeded",
  "external_thread_binding_reconciliation_failed",
  "external_thread_crew_session_reconciliation_failed",
  "external_thread_native_delete_failed",
] as const;

export const EXTERNAL_BINDING_PROFILE_REFRESH_API_REASON_CODES = [
  "external_binding_profile_refresh_invalid_request",
  "external_binding_profile_refresh_not_found",
  "external_binding_profile_refresh_inactive",
  "external_binding_profile_refresh_revision_conflict",
  "external_binding_profile_refresh_identity_conflict",
  "external_binding_profile_refresh_profile_unavailable",
  "external_binding_profile_refresh_profile_revision_conflict",
  "external_binding_profile_refresh_thread_busy",
  "external_binding_profile_refresh_native_failed",
  "external_binding_profile_refresh_persist_failed",
] as const;

export const EXTERNAL_BINDING_RESTORE_API_REASON_CODES = [
  "external_binding_restore_invalid_request",
  "external_binding_restore_not_found",
  "external_binding_restore_revision_conflict",
  "external_binding_restore_identity_conflict",
  "external_binding_restore_status_conflict",
  "external_binding_restore_work_conflict",
  "external_binding_restore_session_config_missing",
  "external_binding_restore_runtime_unavailable",
  "external_binding_restore_profile_missing",
  "external_binding_restore_profile_inactive",
  "external_binding_restore_prompt_conflict",
  "external_binding_restore_session_status_conflict",
  "external_binding_restore_native_thread_missing",
  "external_binding_restore_native_lookup_failed",
  "external_binding_restore_native_unarchive_failed",
  "external_binding_restore_native_resume_failed",
  "external_binding_restore_native_compensation_failed",
  "external_binding_restore_session_persist_failed",
  "external_binding_restore_binding_persist_failed",
] as const;

export const EXTERNAL_CONTROL_API_REASON_CODES = [
  "external_control_idempotency_conflict",
  "external_control_binding_not_found",
  "external_control_binding_revision_conflict",
  "external_control_binding_inactive",
  "external_control_native_turn_required",
  "external_control_thread_unbound",
  "external_control_thread_busy",
  "external_control_native_turn_conflict",
  "external_control_invalid_request",
  "external_control_rejected",
  "external_control_submission_failed",
] as const;

export const EXTERNAL_RUNTIME_API_OPENAPI_PATH =
  "docs/external-runtime-api-v0.openapi.json";

export const EXTERNAL_RUNTIME_API_PATHS = {
  agentSessions: "/v1/external-agent-sessions",
  runtimes: "/v1/external-runtimes",
  promotionReadiness: "/v1/admin/external-runtime-promotion-readiness",
  certifications: "/v1/admin/external-runtime-certifications",
  certification: "/v1/admin/external-runtime-certifications/{certification_id}",
  certificationInvalidate:
    "/v1/admin/external-runtime-certifications/{certification_id}/invalidate",
  runtime: "/v1/external-runtimes/{runtime_id}",
  connect: "/v1/external-runtimes/{runtime_id}/connect",
  threads: "/v1/external-runtimes/{runtime_id}/threads",
  threadRead: "/v1/external-runtimes/{runtime_id}/threads/read",
  threadArchive:
    "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/archive",
  threadDelete: "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/delete",
  threadUnarchive:
    "/v1/external-runtimes/{runtime_id}/threads/{thread_id}/unarchive",
  events: "/v1/external-runtimes/{runtime_id}/events",
  eventHead: "/v1/external-runtimes/{runtime_id}/events/head",
  stream: "/v1/external-runtimes/{runtime_id}/stream",
  rawDetail: "/v1/external-runtimes/{runtime_id}/raw-details/{detail_id}",
  bindings: "/v1/external-bindings",
  bindingRestore: "/v1/external-bindings/{binding_id}/restore",
  bindingProfileRefresh: "/v1/external-bindings/{binding_id}/profile-refresh",
  bindingMetadata: "/v1/external-bindings/{binding_id}/metadata",
  controls: "/v1/external-bindings/{binding_id}/controls",
  commands: "/v1/external-bindings/{binding_id}/commands",
  messages: "/v1/external-bindings/{binding_id}/messages",
  interactions: "/v1/external-interactions",
  interactionResolve: "/v1/external-interactions/{interaction_id}/resolve",
  turn: "/v1/external-turns/{request_id}",
  delivery: "/v1/agent-deliveries/{delivery_id}",
  round: "/v1/agent-rounds/{round_id}",
} as const;

export interface ExternalThreadItemProjection {
  readonly itemId: string;
  readonly kind: string;
  readonly status?: string;
  readonly text?: string;
  readonly summary?: readonly string[];
  readonly messagePhase?: ExternalAgentMessagePhase;
  readonly inputImages?: readonly ExternalInputImageReference[];
  readonly detailHandle?: string;
  readonly truncated?: boolean;
}

export interface ExternalInputImageReference {
  readonly attachmentId: string;
  readonly filename: string;
  readonly mimeType: string;
  readonly byteSize: number;
  readonly sha256: string | null;
  readonly contentUrl: string;
}

export type ExternalAgentMessagePhase =
  | "commentary"
  | "final_answer"
  | "unknown";

export interface ExternalThreadTurnProjection {
  readonly turnId: string;
  readonly status: string;
  readonly statusSource: "native" | "crew_terminal";
  readonly terminalReasonCode: string | null;
  readonly error: ExternalThreadTurnErrorProjection | null;
  readonly startedAt: number | null;
  readonly completedAt: number | null;
  readonly durationMs: number | null;
  readonly items: readonly ExternalThreadItemProjection[];
  readonly itemsTruncated?: boolean;
}

export interface ExternalThreadTurnErrorProjection {
  readonly message: string;
  readonly code: string | null;
  readonly additionalDetails: string | null;
  readonly willRetry: boolean | null;
}

export interface ExternalThreadProjection {
  readonly threadId: string;
  readonly sessionId: string;
  readonly bindingId: string | null;
  readonly crewSessionId: string | null;
  readonly lineage: ExternalAgentBindingLineage | null;
  readonly nativeMaterialized: boolean;
  readonly parentThreadId: string | null;
  readonly preview: string;
  readonly ephemeral: boolean;
  readonly modelProvider: string;
  /** Exact model Codex will use for the next turn, or null while unavailable. */
  readonly effectiveModel: string | null;
  readonly createdAt: number;
  readonly updatedAt: number;
  readonly status: string;
  readonly cwd: string;
  readonly cliVersion: string;
  readonly name: string | null;
  readonly agentNickname: string | null;
  readonly agentRole: string | null;
  readonly turns: readonly ExternalThreadTurnProjection[];
}

export interface ExternalThreadPage {
  readonly items: readonly ExternalThreadProjection[];
  readonly nextCursor: string | null;
  readonly backwardsCursor: string | null;
}

export interface ExternalThreadReadResult {
  readonly thread: ExternalThreadProjection;
  readonly turnPage: ExternalThreadTurnPage;
}

export interface ExternalThreadTurnPage {
  readonly limit: number;
  readonly hasMoreBefore: boolean;
  readonly beforeCursor: string | null;
  readonly pageStartCursor: string | null;
  readonly pageEndCursor: string | null;
}

export interface ExternalThreadLifecycleBindingTransition {
  readonly bindingId: string;
  readonly previousStatus: string;
  readonly currentStatus: string;
  readonly revision: number;
}

export interface ExternalThreadLifecycleSessionTransition {
  readonly sessionId: string;
  readonly previousStatus: string;
  readonly currentStatus: string;
}

export interface ExternalThreadLifecycleReceipt {
  readonly runtimeId: string;
  readonly threadId: string;
  readonly action: "archive" | "unarchive";
  readonly outcome: "applied" | "already_archived" | "already_active";
  readonly nativeArchived: boolean;
  readonly bindings: readonly ExternalThreadLifecycleBindingTransition[];
  readonly crewSessions: readonly ExternalThreadLifecycleSessionTransition[];
}

export interface ExternalThreadDeleteReceipt {
  readonly runtimeId: string;
  readonly threadId: string;
  readonly action: "delete";
  readonly outcome: "applied" | "already_deleted";
  readonly nativeDeleted: true;
  readonly bindings: readonly ExternalThreadLifecycleBindingTransition[];
}

export interface ExternalAgentSessionCreateResult {
  readonly creation: ExternalAgentSessionCreationRecord;
  readonly runtime: ExternalRuntimeRegistration;
  readonly thread: ExternalThreadProjection;
}

export interface ExternalRuntimeCommandDescriptor {
  readonly name: string;
  readonly aliases: readonly string[];
  readonly usage: string;
  readonly description: string;
  readonly mutates: boolean;
  readonly requiredCapabilities: readonly string[];
  readonly available: boolean;
  readonly unavailableReasonCode: string | null;
}

export interface ExternalRuntimeReasoningEffortOption {
  readonly value: string;
  readonly description: string;
}

export interface ExternalRuntimeModelOption {
  readonly id: string;
  readonly model: string;
  readonly displayName: string;
  readonly description: string;
  readonly hidden: boolean;
  readonly isDefault: boolean;
  readonly defaultEffort: string;
  readonly supportedEfforts: readonly ExternalRuntimeReasoningEffortOption[];
}

export interface ExternalThreadSettingsProjection {
  readonly model: string;
  readonly modelProvider: string;
  readonly effort: string | null;
}

export interface ExternalThreadUsageProjection {
  readonly total: Readonly<Record<string, number>>;
  readonly last: Readonly<Record<string, number>>;
  readonly modelContextWindow: number | null;
  readonly contextWindowUsedPercent: number | null;
}

export interface ExternalRuntimeControllerStatus {
  readonly runtimeId: string;
  readonly driverState: string;
  readonly controllerInstanceId: string;
  readonly controllerGeneration: number;
  readonly leaseExpiresAt: string;
  readonly observedCliVersion: string | null;
  readonly consumedContractRevision: string | null;
  readonly compatibilityState: ExternalRuntimeRegistration["compatibilityState"];
  readonly lastCompatibilityProbe: ExternalRuntimeRegistration["lastCompatibilityProbe"];
  readonly recovery: {
    readonly phase:
      | "idle"
      | "scheduled"
      | "attempting"
      | "succeeded"
      | "failed";
    readonly totalAttempts: number;
    readonly consecutiveFailures: number;
    readonly lastAttemptAt: string | null;
    readonly lastRecoveredAt: string | null;
    readonly nextAttemptAt: string | null;
    readonly lastFailureReason: string | null;
  };
  readonly bindingResumeFailures: readonly {
    readonly bindingId: string;
    readonly nativeThreadId: string;
    readonly reason: string;
    readonly observedAt: string;
  }[];
}

export interface ExternalThreadCommandStatus {
  readonly runtimeId: string;
  readonly runtimeKind: string;
  readonly runtimeObservedState: string;
  readonly controller: ExternalRuntimeControllerStatus;
  readonly bindingId: string;
  readonly bindingRevision: number;
  readonly bindingStatus: string;
  readonly sessionId: string | null;
  readonly agentId: string | null;
  readonly nativeThreadId: string;
  readonly activeNativeTurnId: string | null;
  readonly settings: ExternalThreadSettingsProjection;
  readonly usage: ExternalThreadUsageProjection | null;
}

export interface ExternalRuntimeCommandCatalog {
  readonly contractVersion: string;
  readonly runtimeId: string;
  readonly bindingId: string;
  readonly nativeThreadId: string;
  readonly commands: readonly ExternalRuntimeCommandDescriptor[];
  readonly settings: ExternalThreadSettingsProjection;
  readonly models: readonly ExternalRuntimeModelOption[];
}

export interface ExternalRuntimeCommandResultData {
  readonly catalog?: ExternalRuntimeCommandCatalog;
  readonly status?: ExternalThreadCommandStatus;
  readonly settings?: ExternalThreadSettingsProjection;
  readonly models?: readonly ExternalRuntimeModelOption[];
  readonly validEfforts?: readonly ExternalRuntimeReasoningEffortOption[];
  readonly threadReplacement?: ExternalRuntimeThreadReplacementResult;
  readonly nativeResult?: unknown;
}

export interface ExternalRuntimeThreadReplacementResult {
  readonly bindingId: string;
  readonly bindingRevision: number;
  readonly sessionId: string | null;
  readonly profileId: string | null;
  readonly cwd: string;
  readonly label: string | null;
  readonly taskRef: DenRuntimeReference | null;
  readonly previousBindingId: string;
  readonly previousSessionId: string | null;
  readonly previousNativeThreadId: string;
  readonly nativeThreadId: string;
  readonly previousNativeThreadArchived: boolean;
  readonly settingsPreserved: boolean;
  readonly settings: ExternalThreadSettingsProjection;
}

export interface ExternalRuntimeCommandExecutionResult {
  readonly commandId: string;
  readonly input: string;
  readonly command: string;
  readonly argument: string | null;
  readonly status: ExternalControlReceipt["status"];
  readonly reasonCode: string | null;
  readonly message: string;
  readonly result: ExternalRuntimeCommandResultData;
  readonly receipt: ExternalControlReceipt;
}

type JsonSchema = Record<string, unknown> | boolean;

interface OperationContract {
  readonly capabilityId: string;
  readonly operationId: string;
  readonly method: "get" | "post";
  readonly path: string;
  readonly responseSchema?: string;
  readonly requestSchema?: string;
  readonly query?: readonly QueryParameter[];
  readonly sse?: boolean;
  readonly errorReasonCodes?: readonly string[];
}

interface QueryParameter {
  readonly name: string;
  readonly schema: JsonSchema;
  readonly required?: boolean;
}

export const EXTERNAL_RUNTIME_API_OPERATIONS = [
  operation(
    "external.agent_sessions.create",
    "createExternalAgentSession",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.agentSessions,
    "ExternalAgentSessionCreateResult",
    "ExternalAgentSessionCreateWrite",
  ),
  operation(
    "external.runtimes.list",
    "listExternalRuntimes",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.runtimes,
    "ExternalRuntimeFleet",
  ),
  operation(
    "external.runtimes.register",
    "registerExternalRuntime",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.runtimes,
    "ExternalRuntimeRegistration",
    "ExternalRuntimeRegistrationWrite",
  ),
  operation(
    "external.runtimes.read",
    "readExternalRuntime",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.runtime,
    "ExternalRuntimeDetail",
  ),
  operation(
    "external.runtimes.promotion_readiness",
    "readExternalRuntimePromotionReadiness",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.promotionReadiness,
    "ExternalRuntimePromotionReadiness",
    undefined,
    [
      {
        name: "runtimeId",
        schema: { type: "string", minLength: 1 },
        required: true,
      },
    ],
  ),
  operation(
    "external.runtimes.certifications.list",
    "listExternalRuntimeCertifications",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.certifications,
    "ExternalRuntimeCertificationList",
  ),
  operation(
    "external.runtimes.certifications.create",
    "certifyExternalRuntime",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.certifications,
    "ExternalRuntimeCertificationRecord",
    "ExternalRuntimeCertificationWrite",
  ),
  operation(
    "external.runtimes.certifications.read",
    "readExternalRuntimeCertification",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.certification,
    "ExternalRuntimeCertificationRecord",
  ),
  operation(
    "external.runtimes.certifications.invalidate",
    "invalidateExternalRuntimeCertification",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.certificationInvalidate,
    "ExternalRuntimeCertificationRecord",
    "ExternalRuntimeCertificationInvalidationWrite",
  ),
  operation(
    "external.runtimes.connect",
    "connectExternalRuntime",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.connect,
    "ExternalRuntimeControllerStatus",
  ),
  operation(
    "external.runtimes.threads.list",
    "listExternalRuntimeThreads",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.threads,
    "ExternalThreadPage",
    undefined,
    [
      { name: "cursor", schema: { type: "string" } },
      {
        name: "limit",
        schema: { type: "integer", minimum: 1, maximum: 1000, default: 50 },
      },
      { name: "archived", schema: { type: "boolean", default: false } },
    ],
  ),
  operation(
    "external.runtimes.threads.read",
    "readExternalRuntimeThread",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.threadRead,
    "ExternalThreadReadResult",
    "ExternalThreadReadRequest",
    undefined,
    EXTERNAL_THREAD_READ_API_REASON_CODES,
  ),
  operation(
    "external.runtimes.threads.archive",
    "archiveExternalRuntimeThread",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.threadArchive,
    "ExternalThreadLifecycleReceipt",
    undefined,
    undefined,
    EXTERNAL_THREAD_LIFECYCLE_API_REASON_CODES,
  ),
  operation(
    "external.runtimes.threads.delete",
    "deleteExternalRuntimeThread",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.threadDelete,
    "ExternalThreadDeleteReceipt",
    undefined,
    undefined,
    EXTERNAL_THREAD_LIFECYCLE_API_REASON_CODES,
  ),
  operation(
    "external.runtimes.threads.unarchive",
    "unarchiveExternalRuntimeThread",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.threadUnarchive,
    "ExternalThreadLifecycleReceipt",
    undefined,
    undefined,
    EXTERNAL_THREAD_LIFECYCLE_API_REASON_CODES,
  ),
  operation(
    "external.runtimes.events.list",
    "listExternalRuntimeEvents",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.events,
    "ExternalRuntimeEventPage",
    undefined,
    [
      { name: "after", schema: { type: "integer", minimum: 0, default: 0 } },
      { name: "native_thread_id", schema: { type: "string", minLength: 1 } },
      {
        name: "limit",
        schema: { type: "integer", minimum: 1, maximum: 1000, default: 200 },
      },
    ],
  ),
  operation(
    "external.runtimes.events.head",
    "readExternalRuntimeEventHead",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.eventHead,
    "ExternalRuntimeEventHead",
  ),
  {
    ...operation(
      "external.runtimes.events.stream",
      "streamExternalRuntimeEvents",
      "get",
      EXTERNAL_RUNTIME_API_PATHS.stream,
    ),
    sse: true,
  },
  operation(
    "external.runtimes.raw_details.read",
    "readExternalRuntimeRawDetail",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.rawDetail,
    "ExternalRuntimeRawDetail",
  ),
  operation(
    "external.bindings.list",
    "listExternalBindings",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.bindings,
    "ExternalBindingFleet",
  ),
  operation(
    "external.bindings.write",
    "writeExternalBinding",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.bindings,
    "ExternalAgentBinding",
    "ExternalBindingWrite",
  ),
  operation(
    "external.bindings.metadata.write",
    "writeExternalBindingMetadata",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.bindingMetadata,
    "ExternalAgentBinding",
    "ExternalBindingMetadataWrite",
  ),
  operation(
    "external.bindings.restore",
    "restoreExternalBinding",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.bindingRestore,
    "ExternalAgentBindingRestoreReceipt",
    "ExternalBindingRestoreWrite",
    undefined,
    EXTERNAL_BINDING_RESTORE_API_REASON_CODES,
  ),
  operation(
    "external.bindings.profile.refresh",
    "refreshExternalBindingProfile",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.bindingProfileRefresh,
    "ExternalBindingProfileRefreshReceipt",
    "ExternalBindingProfileRefreshWrite",
    undefined,
    EXTERNAL_BINDING_PROFILE_REFRESH_API_REASON_CODES,
  ),
  operation(
    "external.bindings.control",
    "submitExternalBindingControl",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.controls,
    "ExternalControlReceipt",
    "ExternalControlWrite",
    undefined,
    EXTERNAL_CONTROL_API_REASON_CODES,
  ),
  operation(
    "external.bindings.commands.list",
    "listExternalBindingCommands",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.commands,
    "ExternalRuntimeCommandCatalog",
  ),
  operation(
    "external.bindings.commands.execute",
    "executeExternalBindingCommand",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.commands,
    "ExternalRuntimeCommandExecutionResult",
    "ExternalRuntimeCommandWrite",
  ),
  operation(
    "external.bindings.messages.create",
    "sendExternalBindingMessage",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.messages,
    "AgentMessageDeliveryReceipt",
    "ExternalBindingMessageWrite",
  ),
  operation(
    "external.interactions.list",
    "listExternalInteractions",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.interactions,
    "ExternalInteractionAttention",
  ),
  operation(
    "external.interactions.resolve",
    "resolveExternalInteraction",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.interactionResolve,
    "ExternalInteractionRecord",
    "ExternalInteractionResolutionWrite",
  ),
  operation(
    "external.turns.read",
    "readExternalTurn",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.turn,
    "ExternalTurnCorrelation",
  ),
  operation(
    "agent.deliveries.read",
    "readAgentDelivery",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.delivery,
    "AgentMessageDeliveryReceipt",
  ),
  operation(
    "agent.rounds.read",
    "readAgentRound",
    "get",
    EXTERNAL_RUNTIME_API_PATHS.round,
    "AgentCorrelatedRound",
  ),
] as const satisfies readonly OperationContract[];

export function externalRuntimeApiOpenApiDocument(input: {
  readonly coreProtocolSchemas: Readonly<Record<string, JsonSchema>>;
  readonly capabilityIds: ReadonlySet<string>;
}): Record<string, unknown> {
  for (const operation of EXTERNAL_RUNTIME_API_OPERATIONS) {
    if (!input.capabilityIds.has(operation.capabilityId)) {
      throw new Error(
        `external runtime OpenAPI operation has no capability: ${operation.capabilityId}`,
      );
    }
  }
  const paths: Record<string, Record<string, unknown>> = {};
  for (const operation of EXTERNAL_RUNTIME_API_OPERATIONS) {
    const pathItem = (paths[operation.path] ??= {});
    pathItem[operation.method] = openApiOperation(operation);
  }
  return {
    openapi: "3.1.0",
    info: {
      title: "Rusty Crew External Agent Runtime API",
      version: EXTERNAL_RUNTIME_API_CONTRACT_VERSION,
      description:
        "Generated browser-safe contract for runtime-neutral external agent fleet, thread, event, control, interaction, delivery, and round operations.",
    },
    security: [{ AdminBearer: [] }],
    paths,
    components: {
      securitySchemes: {
        AdminBearer: { type: "http", scheme: "bearer" },
      },
      schemas: {
        ...rewriteCoreSchemaReferences(input.coreProtocolSchemas),
        ...routeSchemas(),
      },
    },
    "x-rusty-crew-generated": {
      source: "ts/packages/brain-island/src/external-runtime-api-contract.ts",
      rust_protocol_source:
        "ts/packages/contracts/src/generated/core-protocol.schema.json",
      generator:
        "ts/packages/brain-island/src/generate-api-capability-artifact.ts",
      regeneration_command: "npm run codegen:api-capabilities",
      operation_count: EXTERNAL_RUNTIME_API_OPERATIONS.length,
      native_payload_policy: "bounded_raw_detail_only",
    },
  };
}

function operation(
  capabilityId: string,
  operationId: string,
  method: "get" | "post",
  path: string,
  responseSchema?: string,
  requestSchema?: string,
  query?: readonly QueryParameter[],
  errorReasonCodes?: readonly string[],
): OperationContract {
  return {
    capabilityId,
    operationId,
    method,
    path,
    ...(responseSchema === undefined ? {} : { responseSchema }),
    ...(requestSchema === undefined ? {} : { requestSchema }),
    ...(query === undefined ? {} : { query }),
    ...(errorReasonCodes === undefined ? {} : { errorReasonCodes }),
  };
}

function openApiOperation(
  operation: OperationContract,
): Record<string, unknown> {
  const parameters = [
    ...pathParameters(operation.path),
    ...(operation.query ?? []).map((parameter) => ({
      name: parameter.name,
      in: "query",
      required: parameter.required ?? false,
      schema: parameter.schema,
    })),
  ];
  const responses = operation.sse
    ? {
        "200": {
          description: "Cursor-ordered normalized external runtime events",
          content: {
            "text/event-stream": {
              schema: { type: "string" },
              "x-rusty-crew-event-schema": {
                $ref: "#/components/schemas/NormalizedExternalRuntimeEvent",
              },
            },
          },
        },
        default: errorResponse(),
      }
    : {
        "200": {
          description: "Successful typed response",
          content: {
            "application/json": {
              schema: successEnvelopeSchema(operation.responseSchema!),
            },
          },
        },
        default: errorResponse(),
      };
  return {
    operationId: operation.operationId,
    tags: ["external-runtime"],
    ...(parameters.length === 0 ? {} : { parameters }),
    ...(operation.requestSchema === undefined
      ? {}
      : {
          requestBody: {
            required: true,
            content: {
              "application/json": {
                schema: {
                  $ref: `#/components/schemas/${operation.requestSchema}`,
                },
              },
            },
          },
        }),
    responses,
    "x-rusty-crew-capability-id": operation.capabilityId,
    "x-rusty-crew-contract-detail": "wire",
    ...(operation.errorReasonCodes === undefined
      ? {}
      : { "x-rusty-crew-error-reason-codes": operation.errorReasonCodes }),
  };
}

function pathParameters(path: string): Record<string, unknown>[] {
  return [...path.matchAll(/\{([^}]+)\}/g)].map((match) => ({
    name: match[1],
    in: "path",
    required: true,
    schema: { type: "string", minLength: 1 },
  }));
}

function successEnvelopeSchema(schemaName: string): JsonSchema {
  return {
    type: "object",
    required: ["ok", "data", "meta"],
    properties: {
      ok: { const: true },
      data: { $ref: `#/components/schemas/${schemaName}` },
      meta: { $ref: "#/components/schemas/ApiMeta" },
    },
    additionalProperties: false,
  };
}

function errorResponse(): Record<string, unknown> {
  return {
    description: "Rusty Crew API error envelope",
    content: {
      "application/json": {
        schema: { $ref: "#/components/schemas/ApiErrorEnvelope" },
      },
    },
  };
}

function rewriteCoreSchemaReferences(
  schemas: Readonly<Record<string, JsonSchema>>,
): Record<string, JsonSchema> {
  return Object.fromEntries(
    Object.entries(schemas).map(([name, schema]) => [
      name,
      rewriteReferences(schema) as JsonSchema,
    ]),
  );
}

function rewriteReferences(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(rewriteReferences);
  if (typeof value !== "object" || value === null) return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, child]) => [
      key,
      key === "$ref" && typeof child === "string"
        ? child.replace("#/$defs/", "#/components/schemas/")
        : rewriteReferences(child),
    ]),
  );
}

function routeSchemas(): Record<string, JsonSchema> {
  const nullableString = { type: ["string", "null"] };
  const nullableInteger = { type: ["integer", "null"] };
  return {
    ExternalRuntimeCertificationList: {
      type: "object",
      required: ["certifications"],
      properties: {
        certifications: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeCertificationRecord",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimePromotionReadiness: {
      type: "object",
      required: [
        "registration",
        "controller",
        "activeBindings",
        "activeTurns",
        "pendingInteractions",
      ],
      properties: {
        registration: {
          $ref: "#/components/schemas/ExternalRuntimeRegistration",
        },
        controller: {
          anyOf: [
            { $ref: "#/components/schemas/ExternalRuntimeControllerStatus" },
            { type: "null" },
          ],
        },
        activeBindings: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalAgentBinding" },
        },
        activeTurns: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalTurnCorrelation" },
        },
        pendingInteractions: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalInteractionRecord" },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCertificationWrite: {
      type: "object",
      required: [
        "certificationId",
        "idempotencyKey",
        "runtimeId",
        "evidenceSummary",
      ],
      properties: {
        certificationId: { type: "string", minLength: 1, maxLength: 256 },
        idempotencyKey: { type: "string", minLength: 1, maxLength: 256 },
        runtimeId: { type: "string", minLength: 1, maxLength: 256 },
        evidenceSummary: { type: "string", minLength: 1, maxLength: 4096 },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCertificationInvalidationWrite: {
      type: "object",
      required: ["expectedRevision", "reason"],
      properties: {
        expectedRevision: { type: "integer", minimum: 1 },
        reason: { type: "string", minLength: 1, maxLength: 1024 },
      },
      additionalProperties: false,
    },
    ApiMeta: {
      type: "object",
      required: ["request_id", "schema_version"],
      properties: {
        request_id: { type: "string" },
        schema_version: { type: "integer", const: 1 },
      },
      additionalProperties: false,
    },
    ApiError: {
      type: "object",
      required: ["code", "reason_code", "message", "retryable"],
      properties: {
        code: {
          type: "string",
          enum: [
            "unauthorized",
            "forbidden",
            "method_not_allowed",
            "not_found",
            "invalid_input",
            "failed_precondition",
            "conflict",
            "internal_error",
          ],
        },
        reason_code: { type: "string" },
        message: { type: "string" },
        retryable: { type: "boolean" },
      },
      additionalProperties: false,
    },
    ApiErrorEnvelope: {
      type: "object",
      required: ["ok", "error", "meta"],
      properties: {
        ok: { const: false },
        error: { $ref: "#/components/schemas/ApiError" },
        meta: { $ref: "#/components/schemas/ApiMeta" },
      },
      additionalProperties: false,
    },
    ExternalRuntimeControllerStatus: {
      type: "object",
      required: [
        "runtimeId",
        "driverState",
        "controllerInstanceId",
        "controllerGeneration",
        "leaseExpiresAt",
        "observedCliVersion",
        "consumedContractRevision",
        "compatibilityState",
        "compatibilityDiagnostic",
        "lastCompatibilityProbe",
        "recovery",
        "bindingResumeFailures",
      ],
      properties: {
        runtimeId: { type: "string" },
        driverState: { type: "string" },
        controllerInstanceId: { type: "string" },
        controllerGeneration: { type: "integer", minimum: 0 },
        leaseExpiresAt: { type: "string", format: "date-time" },
        observedCliVersion: { type: ["string", "null"] },
        consumedContractRevision: { type: ["string", "null"] },
        compatibilityState: {
          $ref: "#/components/schemas/ExternalRuntimeCompatibilityState",
        },
        compatibilityDiagnostic: {
          type: "string",
          enum: [
            "certified",
            "compatible_uncertified",
            "incompatible",
            "probe_failed",
            "disconnected",
          ],
        },
        lastCompatibilityProbe: {
          oneOf: [
            {
              $ref: "#/components/schemas/ExternalRuntimeCompatibilityProbeReport",
            },
            { type: "null" },
          ],
        },
        recovery: {
          type: "object",
          required: [
            "phase",
            "totalAttempts",
            "consecutiveFailures",
            "lastAttemptAt",
            "lastRecoveredAt",
            "nextAttemptAt",
            "lastFailureReason",
          ],
          properties: {
            phase: {
              type: "string",
              enum: ["idle", "scheduled", "attempting", "succeeded", "failed"],
            },
            totalAttempts: { type: "integer", minimum: 0 },
            consecutiveFailures: { type: "integer", minimum: 0 },
            lastAttemptAt: { type: ["string", "null"], format: "date-time" },
            lastRecoveredAt: {
              type: ["string", "null"],
              format: "date-time",
            },
            nextAttemptAt: { type: ["string", "null"], format: "date-time" },
            lastFailureReason: { type: ["string", "null"] },
          },
          additionalProperties: false,
        },
        bindingResumeFailures: {
          type: "array",
          items: {
            type: "object",
            required: ["bindingId", "nativeThreadId", "reason", "observedAt"],
            properties: {
              bindingId: { type: "string" },
              nativeThreadId: { type: "string" },
              reason: { type: "string" },
              observedAt: { type: "string", format: "date-time" },
            },
            additionalProperties: false,
          },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeFleet: {
      type: "object",
      required: ["runtimes", "controllers"],
      properties: {
        runtimes: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalRuntimeRegistration" },
        },
        controllers: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeControllerStatus",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeDetail: {
      type: "object",
      required: ["registration"],
      properties: {
        registration: {
          $ref: "#/components/schemas/ExternalRuntimeRegistration",
        },
        controller: {
          anyOf: [
            { $ref: "#/components/schemas/ExternalRuntimeControllerStatus" },
            { type: "null" },
          ],
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeRegistrationWrite: {
      type: "object",
      required: ["registration"],
      properties: {
        registration: {
          $ref: "#/components/schemas/ExternalRuntimeRegistration",
        },
        expectedRevision: { type: "integer", minimum: 0 },
      },
      additionalProperties: false,
    },
    ExternalAgentSessionCreateWrite: {
      type: "object",
      required: ["idempotencyKey", "runtimeId", "profileId", "cwd"],
      properties: {
        idempotencyKey: { type: "string", minLength: 1, maxLength: 256 },
        runtimeId: { type: "string", minLength: 1, maxLength: 256 },
        profileId: { type: "string", minLength: 1, maxLength: 256 },
        cwd: { type: "string", minLength: 1, maxLength: 4096 },
        taskRef: { $ref: "#/components/schemas/DenRuntimeReference" },
        label: { type: "string", minLength: 1, maxLength: 256 },
      },
      additionalProperties: false,
    },
    ExternalAgentSessionCreateResult: {
      type: "object",
      required: ["creation", "runtime", "thread"],
      properties: {
        creation: {
          $ref: "#/components/schemas/ExternalAgentSessionCreationRecord",
        },
        runtime: {
          $ref: "#/components/schemas/ExternalRuntimeRegistration",
        },
        thread: { $ref: "#/components/schemas/ExternalThreadProjection" },
      },
      additionalProperties: false,
    },
    ExternalBindingFleet: {
      type: "object",
      required: ["bindings", "profileStates"],
      properties: {
        bindings: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalAgentBinding" },
        },
        profileStates: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalBindingProfileState",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalBindingWrite: {
      type: "object",
      required: ["binding"],
      properties: {
        binding: { $ref: "#/components/schemas/ExternalAgentBinding" },
        expectedRevision: { type: "integer", minimum: 0 },
      },
      additionalProperties: false,
    },
    ExternalBindingMetadataWrite: {
      type: "object",
      required: ["expectedRevision", "label", "taskRef"],
      properties: {
        expectedRevision: { type: "integer", minimum: 1 },
        label: {
          anyOf: [
            { type: "string", minLength: 1, maxLength: 256 },
            { type: "null" },
          ],
        },
        taskRef: {
          anyOf: [
            { $ref: "#/components/schemas/DenRuntimeReference" },
            { type: "null" },
          ],
        },
      },
      additionalProperties: false,
    },
    ExternalBindingRestoreWrite: {
      type: "object",
      required: [
        "expectedBindingRevision",
        "expectedSessionId",
        "expectedAgentId",
        "expectedProfileId",
        "expectedNativeThreadId",
      ],
      properties: {
        expectedBindingRevision: { type: "integer", minimum: 0 },
        expectedSessionId: { type: "string", minLength: 1 },
        expectedAgentId: { type: "string", minLength: 1 },
        expectedProfileId: { type: "string", minLength: 1 },
        expectedNativeThreadId: { type: "string", minLength: 1 },
      },
      additionalProperties: false,
    },
    ExternalBindingProfileState: {
      type: "object",
      required: [
        "bindingId",
        "profileId",
        "state",
        "refreshRequired",
        "appliedProfileRevision",
        "appliedPromptHash",
        "currentProfileRevision",
        "currentPromptHash",
      ],
      properties: {
        bindingId: { type: "string" },
        profileId: nullableString,
        state: {
          type: "string",
          enum: ["unbound", "current", "stale", "profile_unavailable"],
        },
        refreshRequired: { type: "boolean" },
        appliedProfileRevision: nullableInteger,
        appliedPromptHash: nullableString,
        currentProfileRevision: nullableInteger,
        currentPromptHash: nullableString,
      },
      additionalProperties: false,
    },
    ExternalBindingProfileRefreshWrite: {
      type: "object",
      required: [
        "expectedBindingRevision",
        "expectedNativeThreadId",
        "expectedProfileRevision",
        "expectedProfilePromptHash",
      ],
      properties: {
        expectedBindingRevision: { type: "integer", minimum: 0 },
        expectedNativeThreadId: { type: "string", minLength: 1 },
        expectedProfileRevision: { type: "integer", minimum: 1 },
        expectedProfilePromptHash: {
          type: "string",
          minLength: 64,
          maxLength: 64,
        },
      },
      additionalProperties: false,
    },
    ExternalBindingProfileRefreshReceipt: {
      type: "object",
      required: [
        "outcome",
        "binding",
        "previousNativeThreadId",
        "nativeThreadId",
        "previousNativeThreadArchived",
        "profileState",
      ],
      properties: {
        outcome: {
          type: "string",
          enum: ["already_current", "metadata_reconciled", "thread_replaced"],
        },
        binding: { $ref: "#/components/schemas/ExternalAgentBinding" },
        previousNativeThreadId: { type: "string" },
        nativeThreadId: { type: "string" },
        previousNativeThreadArchived: { type: "boolean" },
        profileState: {
          $ref: "#/components/schemas/ExternalBindingProfileState",
        },
      },
      additionalProperties: false,
    },
    ExternalControlWrite: {
      type: "object",
      required: ["kind"],
      properties: {
        controlId: { type: "string" },
        idempotencyKey: { type: "string" },
        expectedBindingRevision: { type: "integer", minimum: 0 },
        expectedNativeTurnId: { type: "string" },
        kind: { $ref: "#/components/schemas/ExternalControlKind" },
        payload: {
          description:
            "Operation-specific payload. interrupt_turn requires an empty object; Crew derives native thread and turn identity from Rust-validated state.",
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCommandWrite: {
      type: "object",
      required: ["input", "idempotencyKey"],
      properties: {
        input: { type: "string", minLength: 1, maxLength: 512 },
        idempotencyKey: { type: "string", minLength: 1, maxLength: 256 },
        expectedBindingRevision: { type: "integer", minimum: 0 },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCommandDescriptor: {
      type: "object",
      required: [
        "name",
        "aliases",
        "usage",
        "description",
        "mutates",
        "requiredCapabilities",
        "available",
        "unavailableReasonCode",
      ],
      properties: {
        name: { type: "string" },
        aliases: { type: "array", items: { type: "string" } },
        usage: { type: "string" },
        description: { type: "string" },
        mutates: { type: "boolean" },
        requiredCapabilities: {
          type: "array",
          items: { type: "string" },
        },
        available: { type: "boolean" },
        unavailableReasonCode: nullableString,
      },
      additionalProperties: false,
    },
    ExternalRuntimeReasoningEffortOption: {
      type: "object",
      required: ["value", "description"],
      properties: {
        value: { type: "string" },
        description: { type: "string" },
      },
      additionalProperties: false,
    },
    ExternalRuntimeModelOption: {
      type: "object",
      required: [
        "id",
        "model",
        "displayName",
        "description",
        "hidden",
        "isDefault",
        "defaultEffort",
        "supportedEfforts",
      ],
      properties: {
        id: { type: "string" },
        model: { type: "string" },
        displayName: { type: "string" },
        description: { type: "string" },
        hidden: { type: "boolean" },
        isDefault: { type: "boolean" },
        defaultEffort: { type: "string" },
        supportedEfforts: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeReasoningEffortOption",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalThreadSettingsProjection: {
      type: "object",
      required: ["model", "modelProvider", "effort"],
      properties: {
        model: { type: "string" },
        modelProvider: { type: "string" },
        effort: nullableString,
      },
      additionalProperties: false,
    },
    ExternalThreadUsageProjection: {
      type: "object",
      required: [
        "total",
        "last",
        "modelContextWindow",
        "contextWindowUsedPercent",
      ],
      properties: {
        total: { type: "object", additionalProperties: { type: "number" } },
        last: { type: "object", additionalProperties: { type: "number" } },
        modelContextWindow: { type: ["number", "null"] },
        contextWindowUsedPercent: { type: ["number", "null"] },
      },
      additionalProperties: false,
    },
    ExternalThreadCommandStatus: {
      type: "object",
      required: [
        "runtimeId",
        "runtimeKind",
        "runtimeObservedState",
        "controller",
        "bindingId",
        "bindingRevision",
        "bindingStatus",
        "sessionId",
        "agentId",
        "nativeThreadId",
        "activeNativeTurnId",
        "settings",
        "usage",
      ],
      properties: {
        runtimeId: { type: "string" },
        runtimeKind: { type: "string" },
        runtimeObservedState: { type: "string" },
        controller: {
          $ref: "#/components/schemas/ExternalRuntimeControllerStatus",
        },
        bindingId: { type: "string" },
        bindingRevision: { type: "integer", minimum: 0 },
        bindingStatus: { type: "string" },
        sessionId: nullableString,
        agentId: nullableString,
        nativeThreadId: { type: "string" },
        activeNativeTurnId: nullableString,
        settings: {
          $ref: "#/components/schemas/ExternalThreadSettingsProjection",
        },
        usage: {
          anyOf: [
            { $ref: "#/components/schemas/ExternalThreadUsageProjection" },
            { type: "null" },
          ],
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCommandCatalog: {
      type: "object",
      required: [
        "contractVersion",
        "runtimeId",
        "bindingId",
        "nativeThreadId",
        "commands",
        "settings",
        "models",
      ],
      properties: {
        contractVersion: { type: "string" },
        runtimeId: { type: "string" },
        bindingId: { type: "string" },
        nativeThreadId: { type: "string" },
        commands: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeCommandDescriptor",
          },
        },
        settings: {
          $ref: "#/components/schemas/ExternalThreadSettingsProjection",
        },
        models: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalRuntimeModelOption" },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCommandResultData: {
      type: "object",
      properties: {
        catalog: {
          $ref: "#/components/schemas/ExternalRuntimeCommandCatalog",
        },
        status: { $ref: "#/components/schemas/ExternalThreadCommandStatus" },
        settings: {
          $ref: "#/components/schemas/ExternalThreadSettingsProjection",
        },
        models: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalRuntimeModelOption" },
        },
        validEfforts: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeReasoningEffortOption",
          },
        },
        threadReplacement: {
          $ref: "#/components/schemas/ExternalRuntimeThreadReplacementResult",
        },
        nativeResult: true,
      },
      additionalProperties: false,
    },
    ExternalRuntimeThreadReplacementResult: {
      type: "object",
      required: [
        "bindingId",
        "bindingRevision",
        "sessionId",
        "profileId",
        "cwd",
        "label",
        "taskRef",
        "previousBindingId",
        "previousSessionId",
        "previousNativeThreadId",
        "nativeThreadId",
        "previousNativeThreadArchived",
        "settingsPreserved",
        "settings",
      ],
      properties: {
        bindingId: { type: "string" },
        bindingRevision: { type: "integer", minimum: 0 },
        sessionId: nullableString,
        profileId: nullableString,
        cwd: { type: "string" },
        label: nullableString,
        taskRef: {
          anyOf: [
            { $ref: "#/components/schemas/DenRuntimeReference" },
            { type: "null" },
          ],
        },
        previousBindingId: { type: "string" },
        previousSessionId: nullableString,
        previousNativeThreadId: { type: "string" },
        nativeThreadId: { type: "string" },
        previousNativeThreadArchived: { type: "boolean" },
        settingsPreserved: { type: "boolean" },
        settings: {
          $ref: "#/components/schemas/ExternalThreadSettingsProjection",
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeCommandExecutionResult: {
      type: "object",
      required: [
        "commandId",
        "input",
        "command",
        "argument",
        "status",
        "reasonCode",
        "message",
        "result",
        "receipt",
      ],
      properties: {
        commandId: { type: "string" },
        input: { type: "string" },
        command: { type: "string" },
        argument: nullableString,
        status: { $ref: "#/components/schemas/ExternalControlStatus" },
        reasonCode: nullableString,
        message: { type: "string" },
        result: {
          $ref: "#/components/schemas/ExternalRuntimeCommandResultData",
        },
        receipt: { $ref: "#/components/schemas/ExternalControlReceipt" },
      },
      additionalProperties: false,
    },
    ExternalBindingMessageWrite: {
      type: "object",
      required: ["body"],
      properties: {
        body: { type: "string", minLength: 1 },
        deliveryId: { type: "string" },
        idempotencyKey: { type: "string" },
        messageId: { type: "string" },
        correlationId: { type: "string" },
        collaborationMode: {
          $ref: "#/components/schemas/ExternalCollaborationMode",
        },
        attachmentIds: {
          type: "array",
          maxItems: 4,
          uniqueItems: true,
          items: { type: "string", minLength: 1 },
        },
        ttlMs: { type: "integer", minimum: 1, maximum: 60000, default: 5000 },
      },
      additionalProperties: false,
    },
    ExternalInteractionAttention: {
      type: "object",
      required: ["interactions"],
      properties: {
        interactions: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalInteractionRecord" },
        },
      },
      additionalProperties: false,
    },
    ExternalInteractionResolutionWrite: {
      type: "object",
      required: ["expectedRevision", "idempotencyKey", "result"],
      properties: {
        expectedRevision: { type: "integer", minimum: 0 },
        idempotencyKey: { type: "string", minLength: 1 },
        result: true,
      },
      additionalProperties: false,
    },
    ExternalRuntimeMediaReference: {
      type: "object",
      required: ["mediaIndex", "captureSource", "captureState"],
      properties: {
        mediaIndex: { type: "integer", minimum: 0 },
        captureSource: {
          type: "string",
          enum: [
            "dynamic_tool_input_image",
            "mcp_image_content",
            "image_view_path",
          ],
        },
        captureState: {
          type: "string",
          enum: [
            "available",
            "unavailable",
            "unsupported",
            "empty",
            "oversized",
            "failed",
          ],
        },
        reasonCode: { type: "string" },
        attachmentId: { type: "string" },
        filename: { type: "string" },
        mimeType: { type: "string" },
        byteSize: { type: "integer", minimum: 0 },
        sha256: { type: "string", pattern: "^[a-f0-9]{64}$" },
        width: { type: "integer", minimum: 1 },
        height: { type: "integer", minimum: 1 },
        contentUrl: { type: "string" },
      },
      additionalProperties: false,
    },
    ExternalRuntimeDocumentReference: {
      type: "object",
      required: ["documentIndex", "captureSource", "captureState"],
      properties: {
        documentIndex: { type: "integer", minimum: 0 },
        captureSource: {
          type: "string",
          enum: ["agent_message_file_link"],
        },
        captureState: {
          type: "string",
          enum: [
            "available",
            "missing",
            "binary",
            "empty",
            "oversized",
            "changed",
            "unsupported",
            "failed",
          ],
        },
        reasonCode: { type: "string" },
        attachmentId: { type: "string" },
        filename: { type: "string" },
        mimeType: { type: "string" },
        languageHint: { type: "string" },
        byteSize: { type: "integer", minimum: 0 },
        sha256: { type: "string", pattern: "^[a-f0-9]{64}$" },
        contentUrl: { type: "string" },
      },
      additionalProperties: false,
    },
    ExternalRuntimeEventPayload: {
      type: "object",
      required: ["nativeMethod"],
      properties: {
        nativeMethod: { type: "string" },
        status: { type: "string" },
        text: { type: "string" },
        message: { type: "string" },
        error: {
          $ref: "#/components/schemas/ExternalThreadTurnErrorProjection",
        },
        command: { type: "string" },
        argument: nullableString,
        controlId: { type: "string" },
        reasonCode: nullableString,
        predecessorBindingId: { type: "string" },
        predecessorSessionId: { type: "string" },
        predecessorNativeThreadId: { type: "string" },
        successorBindingId: { type: "string" },
        successorSessionId: { type: "string" },
        successorNativeThreadId: { type: "string" },
        predecessorLifecycle: {
          type: "string",
          enum: ["retained", "archived"],
        },
        movedRouteCount: { type: "integer", minimum: 0 },
        cwd: { type: "string" },
        output: { type: "string" },
        exitCode: { type: "number" },
        durationMs: { type: "number", minimum: 0 },
        server: { type: "string" },
        tool: { type: "string" },
        success: { type: "boolean" },
        media: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalRuntimeMediaReference" },
        },
        documents: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalRuntimeDocumentReference",
          },
        },
        summary: { type: "array", items: { type: "string" } },
        messagePhase: {
          type: "string",
          enum: ["commentary", "final_answer", "unknown"],
        },
        fileChanges: {
          type: "array",
          items: {
            type: "object",
            properties: {
              path: { type: "string" },
              kind: { type: "string" },
              status: { type: "string" },
            },
            additionalProperties: false,
          },
        },
        settings: {
          $ref: "#/components/schemas/ExternalThreadSettingsProjection",
        },
        usage: {
          type: "object",
          required: ["total", "last", "modelContextWindow"],
          properties: {
            total: {
              type: "object",
              additionalProperties: { type: "number" },
            },
            last: {
              type: "object",
              additionalProperties: { type: "number" },
            },
            modelContextWindow: { type: ["number", "null"] },
          },
          additionalProperties: false,
        },
      },
      additionalProperties: false,
    },
    NormalizedExternalRuntimeEvent: {
      type: "object",
      required: [
        "eventId",
        "runtimeId",
        "sequenceId",
        "createdAt",
        "kind",
        "payload",
      ],
      properties: {
        eventId: { type: "string" },
        runtimeId: { type: "string" },
        sequenceId: { type: "integer", minimum: 0 },
        createdAt: { type: "string", format: "date-time" },
        kind: { type: "string" },
        sessionId: nullableString,
        nativeThreadId: nullableString,
        nativeTurnId: nullableString,
        itemId: nullableString,
        requestId: nullableString,
        rawDetailRef: nullableString,
        payload: { $ref: "#/components/schemas/ExternalRuntimeEventPayload" },
      },
      additionalProperties: false,
    },
    ExternalRuntimeEventPage: {
      type: "object",
      required: ["events"],
      properties: {
        events: {
          type: "array",
          items: {
            $ref: "#/components/schemas/NormalizedExternalRuntimeEvent",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeEventHead: {
      type: "object",
      required: ["event"],
      properties: {
        event: {
          anyOf: [
            { $ref: "#/components/schemas/NormalizedExternalRuntimeEvent" },
            { type: "null" },
          ],
        },
      },
      additionalProperties: false,
    },
    ExternalRuntimeRawDetail: {
      type: "object",
      required: [
        "detailId",
        "runtimeId",
        "json",
        "originalSha256",
        "truncated",
        "redactedKeys",
      ],
      properties: {
        detailId: { type: "string" },
        runtimeId: { type: "string" },
        json: { type: "string" },
        originalSha256: { type: "string" },
        truncated: { type: "boolean" },
        redactedKeys: { type: "array", items: { type: "string" } },
      },
      additionalProperties: false,
    },
    ExternalThreadReadRequest: {
      type: "object",
      required: ["threadId"],
      properties: {
        threadId: { type: "string", minLength: 1 },
        includeTurns: {
          type: "boolean",
          default: true,
          description:
            "When true, returns one bounded turn page. It never requests the complete native transcript.",
        },
        limit: {
          type: "integer",
          minimum: 1,
          maximum: 100,
          default: 50,
        },
        beforeCursor: { type: "string", minLength: 1, maxLength: 2048 },
      },
      additionalProperties: false,
    },
    ExternalThreadItemProjection: {
      type: "object",
      required: ["itemId", "kind"],
      properties: {
        itemId: { type: "string" },
        kind: { type: "string" },
        status: { type: "string" },
        text: { type: "string" },
        summary: { type: "array", items: { type: "string" } },
        messagePhase: {
          type: "string",
          enum: ["commentary", "final_answer", "unknown"],
        },
        inputImages: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalInputImageReference" },
        },
        detailHandle: { type: "string" },
        truncated: { type: "boolean" },
      },
      additionalProperties: false,
    },
    ExternalInputImageReference: {
      type: "object",
      required: [
        "attachmentId",
        "filename",
        "mimeType",
        "byteSize",
        "sha256",
        "contentUrl",
      ],
      properties: {
        attachmentId: { type: "string" },
        filename: { type: "string" },
        mimeType: { type: "string" },
        byteSize: { type: "integer", minimum: 1 },
        sha256: { type: ["string", "null"] },
        contentUrl: { type: "string" },
      },
      additionalProperties: false,
    },
    ExternalThreadTurnProjection: {
      type: "object",
      required: [
        "turnId",
        "status",
        "statusSource",
        "terminalReasonCode",
        "error",
        "startedAt",
        "completedAt",
        "durationMs",
        "items",
      ],
      properties: {
        turnId: { type: "string" },
        status: { type: "string" },
        statusSource: {
          type: "string",
          enum: ["native", "crew_terminal"],
        },
        terminalReasonCode: { type: ["string", "null"] },
        error: {
          anyOf: [
            { $ref: "#/components/schemas/ExternalThreadTurnErrorProjection" },
            { type: "null" },
          ],
        },
        startedAt: { type: ["number", "null"] },
        completedAt: { type: ["number", "null"] },
        durationMs: { type: ["number", "null"] },
        items: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalThreadItemProjection" },
        },
        itemsTruncated: { type: "boolean" },
      },
      additionalProperties: false,
    },
    ExternalThreadTurnErrorProjection: {
      type: "object",
      required: ["message", "code", "additionalDetails", "willRetry"],
      properties: {
        message: { type: "string" },
        code: { type: ["string", "null"] },
        additionalDetails: { type: ["string", "null"] },
        willRetry: { type: ["boolean", "null"] },
      },
      additionalProperties: false,
    },
    ExternalThreadProjection: {
      type: "object",
      required: [
        "threadId",
        "sessionId",
        "bindingId",
        "crewSessionId",
        "lineage",
        "nativeMaterialized",
        "parentThreadId",
        "preview",
        "ephemeral",
        "modelProvider",
        "effectiveModel",
        "createdAt",
        "updatedAt",
        "status",
        "cwd",
        "cliVersion",
        "name",
        "agentNickname",
        "agentRole",
        "turns",
      ],
      properties: {
        threadId: { type: "string" },
        sessionId: { type: "string" },
        bindingId: nullableString,
        crewSessionId: nullableString,
        lineage: {
          anyOf: [
            { $ref: "#/components/schemas/ExternalAgentBindingLineage" },
            { type: "null" },
          ],
        },
        nativeMaterialized: { type: "boolean" },
        parentThreadId: nullableString,
        preview: { type: "string" },
        ephemeral: { type: "boolean" },
        modelProvider: { type: "string" },
        effectiveModel: {
          type: ["string", "null"],
          description:
            "Exact model Codex will use for the next turn. Null means the thread is archived, unloaded, or its authoritative settings are unavailable; clients must not infer a model from modelProvider or other metadata.",
        },
        createdAt: { type: "number" },
        updatedAt: { type: "number" },
        status: { type: "string" },
        cwd: { type: "string" },
        cliVersion: { type: "string" },
        name: nullableString,
        agentNickname: nullableString,
        agentRole: nullableString,
        turns: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalThreadTurnProjection" },
        },
      },
      additionalProperties: false,
    },
    ExternalThreadPage: {
      type: "object",
      required: ["items", "nextCursor", "backwardsCursor"],
      properties: {
        items: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalThreadProjection" },
        },
        nextCursor: nullableString,
        backwardsCursor: nullableString,
      },
      additionalProperties: false,
    },
    ExternalThreadReadResult: {
      type: "object",
      required: ["thread", "turnPage"],
      properties: {
        thread: { $ref: "#/components/schemas/ExternalThreadProjection" },
        turnPage: { $ref: "#/components/schemas/ExternalThreadTurnPage" },
      },
      additionalProperties: false,
    },
    ExternalThreadTurnPage: {
      type: "object",
      required: [
        "limit",
        "hasMoreBefore",
        "beforeCursor",
        "pageStartCursor",
        "pageEndCursor",
      ],
      properties: {
        limit: { type: "integer", minimum: 1, maximum: 100 },
        hasMoreBefore: { type: "boolean" },
        beforeCursor: nullableString,
        pageStartCursor: nullableString,
        pageEndCursor: nullableString,
      },
      additionalProperties: false,
    },
    ExternalThreadLifecycleBindingTransition: {
      type: "object",
      required: ["bindingId", "previousStatus", "currentStatus", "revision"],
      properties: {
        bindingId: { type: "string" },
        previousStatus: { type: "string" },
        currentStatus: { type: "string" },
        revision: { type: "integer", minimum: 0 },
      },
      additionalProperties: false,
    },
    ExternalThreadLifecycleReceipt: {
      type: "object",
      required: [
        "runtimeId",
        "threadId",
        "action",
        "outcome",
        "nativeArchived",
        "bindings",
        "crewSessions",
      ],
      properties: {
        runtimeId: { type: "string" },
        threadId: { type: "string" },
        action: { type: "string", enum: ["archive", "unarchive"] },
        outcome: {
          type: "string",
          enum: ["applied", "already_archived", "already_active"],
        },
        nativeArchived: { type: "boolean" },
        bindings: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalThreadLifecycleBindingTransition",
          },
        },
        crewSessions: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalThreadLifecycleSessionTransition",
          },
        },
      },
      additionalProperties: false,
    },
    ExternalThreadLifecycleSessionTransition: {
      type: "object",
      required: ["sessionId", "previousStatus", "currentStatus"],
      properties: {
        sessionId: { type: "string" },
        previousStatus: { type: "string" },
        currentStatus: { type: "string" },
      },
      additionalProperties: false,
    },
    ExternalThreadDeleteReceipt: {
      type: "object",
      required: [
        "runtimeId",
        "threadId",
        "action",
        "outcome",
        "nativeDeleted",
        "bindings",
      ],
      properties: {
        runtimeId: { type: "string" },
        threadId: { type: "string" },
        action: { type: "string", enum: ["delete"] },
        outcome: {
          type: "string",
          enum: ["applied", "already_deleted"],
        },
        nativeDeleted: { type: "boolean", const: true },
        bindings: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ExternalThreadLifecycleBindingTransition",
          },
        },
      },
      additionalProperties: false,
    },
  };
}
