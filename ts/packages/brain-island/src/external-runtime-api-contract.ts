export const EXTERNAL_RUNTIME_API_CONTRACT_VERSION = "0.1.0";

export const EXTERNAL_RUNTIME_API_OPENAPI_PATH =
  "docs/external-runtime-api-v0.openapi.json";

export const EXTERNAL_RUNTIME_API_PATHS = {
  runtimes: "/v1/external-runtimes",
  runtime: "/v1/external-runtimes/{runtime_id}",
  connect: "/v1/external-runtimes/{runtime_id}/connect",
  threads: "/v1/external-runtimes/{runtime_id}/threads",
  threadRead: "/v1/external-runtimes/{runtime_id}/threads/read",
  events: "/v1/external-runtimes/{runtime_id}/events",
  stream: "/v1/external-runtimes/{runtime_id}/stream",
  rawDetail: "/v1/external-runtimes/{runtime_id}/raw-details/{detail_id}",
  bindings: "/v1/external-bindings",
  controls: "/v1/external-bindings/{binding_id}/controls",
  messages: "/v1/external-bindings/{binding_id}/messages",
  interactions: "/v1/external-interactions",
  interactionResolve: "/v1/external-interactions/{interaction_id}/resolve",
  delivery: "/v1/agent-deliveries/{delivery_id}",
  round: "/v1/agent-rounds/{round_id}",
} as const;

export interface ExternalThreadItemProjection {
  readonly itemId: string;
  readonly kind: string;
  readonly status?: string;
  readonly text?: string;
  readonly summary?: readonly string[];
}

export interface ExternalThreadTurnProjection {
  readonly turnId: string;
  readonly status: string;
  readonly startedAt: number | null;
  readonly completedAt: number | null;
  readonly durationMs: number | null;
  readonly items: readonly ExternalThreadItemProjection[];
}

export interface ExternalThreadProjection {
  readonly threadId: string;
  readonly sessionId: string;
  readonly parentThreadId: string | null;
  readonly preview: string;
  readonly ephemeral: boolean;
  readonly modelProvider: string;
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
}

interface QueryParameter {
  readonly name: string;
  readonly schema: JsonSchema;
}

export const EXTERNAL_RUNTIME_API_OPERATIONS = [
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
    ],
  ),
  operation(
    "external.runtimes.threads.read",
    "readExternalRuntimeThread",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.threadRead,
    "ExternalThreadReadResult",
    "ExternalThreadReadRequest",
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
      {
        name: "limit",
        schema: { type: "integer", minimum: 1, maximum: 1000, default: 200 },
      },
    ],
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
    "external.bindings.control",
    "submitExternalBindingControl",
    "post",
    EXTERNAL_RUNTIME_API_PATHS.controls,
    "ExternalControlReceipt",
    "ExternalControlWrite",
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
): OperationContract {
  return {
    capabilityId,
    operationId,
    method,
    path,
    ...(responseSchema === undefined ? {} : { responseSchema }),
    ...(requestSchema === undefined ? {} : { requestSchema }),
    ...(query === undefined ? {} : { query }),
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
      required: false,
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
  return {
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
      ],
      properties: {
        runtimeId: { type: "string" },
        driverState: { type: "string" },
        controllerInstanceId: { type: "string" },
        controllerGeneration: { type: "integer", minimum: 0 },
        leaseExpiresAt: { type: "string", format: "date-time" },
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
    ExternalBindingFleet: {
      type: "object",
      required: ["bindings"],
      properties: {
        bindings: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalAgentBinding" },
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
    ExternalControlWrite: {
      type: "object",
      required: ["kind"],
      properties: {
        controlId: { type: "string" },
        idempotencyKey: { type: "string" },
        expectedBindingRevision: { type: "integer", minimum: 0 },
        expectedNativeTurnId: { type: "string" },
        kind: { $ref: "#/components/schemas/ExternalControlKind" },
        payload: true,
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
    ExternalRuntimeEventPayload: {
      type: "object",
      required: ["nativeMethod"],
      properties: {
        nativeMethod: { type: "string" },
        status: { type: "string" },
        text: { type: "string" },
        message: { type: "string" },
        command: { type: "string" },
        cwd: { type: "string" },
        output: { type: "string" },
        exitCode: { type: "number" },
        durationMs: { type: "number", minimum: 0 },
        server: { type: "string" },
        tool: { type: "string" },
        success: { type: "boolean" },
        summary: { type: "array", items: { type: "string" } },
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
        usage: {
          type: "object",
          additionalProperties: { type: "number" },
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
        includeTurns: { type: "boolean", default: true },
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
      },
      additionalProperties: false,
    },
    ExternalThreadTurnProjection: {
      type: "object",
      required: [
        "turnId",
        "status",
        "startedAt",
        "completedAt",
        "durationMs",
        "items",
      ],
      properties: {
        turnId: { type: "string" },
        status: { type: "string" },
        startedAt: { type: ["number", "null"] },
        completedAt: { type: ["number", "null"] },
        durationMs: { type: ["number", "null"] },
        items: {
          type: "array",
          items: { $ref: "#/components/schemas/ExternalThreadItemProjection" },
        },
      },
      additionalProperties: false,
    },
    ExternalThreadProjection: {
      type: "object",
      required: [
        "threadId",
        "sessionId",
        "parentThreadId",
        "preview",
        "ephemeral",
        "modelProvider",
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
        parentThreadId: nullableString,
        preview: { type: "string" },
        ephemeral: { type: "boolean" },
        modelProvider: { type: "string" },
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
      required: ["thread"],
      properties: {
        thread: { $ref: "#/components/schemas/ExternalThreadProjection" },
      },
      additionalProperties: false,
    },
  };
}
