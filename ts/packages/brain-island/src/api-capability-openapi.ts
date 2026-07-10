import {
  API_CAPABILITIES,
  type ApiCapabilityDescriptor,
} from "./api-command-registry.js";

type JsonObject = Record<string, unknown>;

const DETAILED_OPERATIONS = {
  "admin.capabilities": {
    operationId: "listApiCapabilities",
    responseSchema: "ApiCapabilityRegistry",
  },
  "chat.commands.list": {
    operationId: "listChatCommands",
    responseSchema: "ChatCommandRegistry",
  },
  "chat.commands.autocomplete": {
    operationId: "autocompleteChatCommandArgument",
    responseSchema: "ChatCommandAutocompleteResult",
  },
} as const satisfies Record<
  string,
  { operationId: string; responseSchema: string }
>;

export const API_CAPABILITY_OPENAPI_PATH =
  "docs/rusty-crew-api-capabilities.openapi.json";

export function apiCapabilityOpenApiDocument(): JsonObject {
  const paths: Record<string, Record<string, JsonObject>> = {};
  const operationIds = new Set<string>();
  for (const capability of [...API_CAPABILITIES]
    .filter((candidate) => candidate.public)
    .sort(compareCapabilities)) {
    const operation = openApiOperation(capability);
    const operationId = operation.operationId;
    if (typeof operationId !== "string") {
      throw new Error(`missing OpenAPI operation id for ${capability.id}`);
    }
    if (operationIds.has(operationId)) {
      throw new Error(`duplicate OpenAPI operation id: ${operationId}`);
    }
    operationIds.add(operationId);
    const pathItem = (paths[capability.path_template] ??= {});
    pathItem[capability.method.toLowerCase()] = operation;
  }

  assertDetailedOperationsExist();
  return {
    openapi: "3.1.0",
    info: {
      title: "Rusty Crew API Capability Contract",
      version: "0.1.0",
      description:
        "Generated public capability inventory. Discovery operations carry detailed wire schemas; other operations intentionally describe capability metadata only.",
    },
    tags: capabilityTags().map((name) => ({ name })),
    paths,
    components: {
      securitySchemes: {
        AdminBearer: { type: "http", scheme: "bearer" },
        ChatBearer: { type: "http", scheme: "bearer" },
      },
      schemas: openApiSchemas(),
    },
    "x-rusty-crew-generated": {
      source: "ts/packages/brain-island/src/api-command-registry.ts",
      generator:
        "ts/packages/brain-island/src/generate-api-capability-artifact.ts",
      regeneration_command: "npm run codegen:api-capabilities",
      capability_count: API_CAPABILITIES.filter(
        (capability) => capability.public,
      ).length,
      detailed_operation_ids: Object.keys(DETAILED_OPERATIONS).sort(),
    },
  };
}

function openApiOperation(capability: ApiCapabilityDescriptor): JsonObject {
  const detailed = detailedOperation(capability.id);
  const parameters = pathParameters(capability.path_template);
  if (capability.id === "chat.commands.autocomplete") {
    parameters.push(
      queryParameter("argument", true),
      queryParameter("query", false),
      {
        name: "limit",
        in: "query",
        required: false,
        schema: { type: "integer", minimum: 1, maximum: 100, default: 20 },
      },
    );
  }

  return {
    operationId: detailed?.operationId ?? operationIdFrom(capability.id),
    summary: capability.description,
    tags: [...capability.tags],
    security: securityFor(capability.auth),
    ...(parameters.length === 0 ? {} : { parameters }),
    responses: responsesFor(detailed),
    "x-rusty-crew-capability-id": capability.id,
    "x-rusty-crew-auth": capability.auth,
    "x-rusty-crew-mutation": capability.mutation,
    "x-rusty-crew-stability": capability.stability,
    "x-rusty-crew-contract-detail": detailed ? "wire" : "capability",
    ...(capability.command_name === undefined
      ? {}
      : { "x-rusty-crew-command-name": capability.command_name }),
    ...(capability.rust_plan_operation === undefined
      ? {}
      : {
          "x-rusty-crew-rust-plan-operation": capability.rust_plan_operation,
        }),
  };
}

function responsesFor(
  detailed: { operationId: string; responseSchema: string } | undefined,
): JsonObject {
  if (!detailed) {
    return {
      default: {
        description:
          "Capability metadata only; status codes and wire body are owned by the operation-specific contract",
      },
    };
  }
  return {
    "200": {
      description: "Successful typed discovery response",
      content: {
        "application/json": {
          schema: successEnvelopeSchema(detailed.responseSchema),
        },
      },
    },
    default: {
      description: "Rusty Crew API error envelope",
      content: {
        "application/json": {
          schema: { $ref: "#/components/schemas/ApiEnvelope" },
        },
      },
    },
  };
}

function openApiSchemas(): Record<string, JsonObject> {
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
    ApiEnvelope: {
      type: "object",
      required: ["ok", "meta"],
      properties: {
        ok: { type: "boolean" },
        data: true,
        error: { $ref: "#/components/schemas/ApiError" },
        meta: { $ref: "#/components/schemas/ApiMeta" },
      },
      additionalProperties: false,
    },
    ApiCapabilityDescriptor: {
      type: "object",
      required: [
        "id",
        "method",
        "path_template",
        "description",
        "auth",
        "mutation",
        "stability",
        "tags",
        "public",
      ],
      properties: {
        id: { type: "string" },
        method: { type: "string", enum: ["DELETE", "GET", "PATCH", "POST"] },
        path_template: { type: "string" },
        description: { type: "string" },
        auth: { type: "string", enum: ["none", "chat", "admin"] },
        mutation: { type: "string", enum: ["read", "write", "control"] },
        stability: { type: "string", enum: ["stable", "experimental"] },
        tags: {
          type: "array",
          items: { type: "string", enum: capabilityTags() },
        },
        public: { type: "boolean" },
        command_name: { type: "string" },
        rust_plan_operation: { type: "string" },
      },
      additionalProperties: false,
    },
    ApiCapabilityRegistry: {
      type: "object",
      required: ["schema_version", "slash_commands", "capabilities"],
      properties: {
        schema_version: { type: "integer", const: 1 },
        slash_commands: {
          type: "array",
          items: { $ref: "#/components/schemas/ChatCommandDescriptor" },
        },
        capabilities: {
          type: "array",
          items: { $ref: "#/components/schemas/ApiCapabilityDescriptor" },
        },
      },
      additionalProperties: false,
    },
    ChatCommandRegistry: {
      type: "object",
      required: ["commands"],
      properties: {
        commands: {
          type: "array",
          items: { $ref: "#/components/schemas/ChatCommandDescriptor" },
        },
      },
      additionalProperties: false,
    },
    ChatCommandDescriptor: {
      type: "object",
      required: [
        "name",
        "aliases",
        "description",
        "args_schema",
        "positional_args",
        "named_args",
        "surfaces",
        "source",
        "read_only",
        "mutating",
        "scope",
        "allowed_session_kinds",
        "requires_control_auth",
      ],
      properties: {
        name: { type: "string" },
        aliases: { type: "array", items: { type: "string" } },
        description: { type: "string" },
        args_schema: { type: "object", additionalProperties: true },
        positional_args: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ChatCommandArgumentDescriptor",
          },
        },
        named_args: {
          type: "array",
          items: {
            $ref: "#/components/schemas/ChatCommandArgumentDescriptor",
          },
        },
        surfaces: {
          type: "array",
          items: { $ref: "#/components/schemas/ChatCommandSurface" },
        },
        source: { $ref: "#/components/schemas/ChatCommandSource" },
        read_only: { type: "boolean" },
        mutating: { type: "boolean" },
        scope: { type: "string", enum: ["session", "profile", "service"] },
        allowed_session_kinds: {
          type: "array",
          items: { type: "string", enum: ["full", "worker", "delegated"] },
        },
        requires_control_auth: { type: "boolean" },
        backing_control_command: { type: "string" },
        rust_plan_operation: { type: "string" },
      },
      additionalProperties: false,
    },
    ChatCommandArgumentDescriptor: {
      type: "object",
      required: ["name", "type", "required"],
      properties: {
        name: { type: "string" },
        description: { type: "string" },
        type: { $ref: "#/components/schemas/ChatCommandArgumentType" },
        required: { type: "boolean" },
        default_value: true,
        enum_values: {
          type: "array",
          items: { $ref: "#/components/schemas/ChatCommandEnumValue" },
        },
        enum_provider: { type: "string" },
        repeated: { type: "boolean" },
        placeholder: { type: "string" },
      },
      additionalProperties: false,
    },
    ChatCommandArgumentType: {
      type: "string",
      enum: ["string", "number", "boolean", "enum", "json", "file"],
    },
    ChatCommandEnumValue: {
      type: "object",
      required: ["value"],
      properties: {
        value: { type: "string" },
        label: { type: "string" },
        description: { type: "string" },
      },
      additionalProperties: false,
    },
    ChatCommandSurface: {
      type: "string",
      enum: ["chat-input", "global", "message-context"],
    },
    ChatCommandSource: {
      type: "string",
      enum: ["backend", "backend-control", "frontend-local", "plugin"],
    },
    ChatCommandAutocompleteResult: {
      type: "object",
      required: ["command_name", "argument_name", "items", "has_more"],
      properties: {
        command_name: { type: "string" },
        argument_name: { type: "string" },
        provider: { type: "string" },
        items: {
          type: "array",
          items: { $ref: "#/components/schemas/ChatCommandEnumValue" },
        },
        has_more: { type: "boolean" },
      },
      additionalProperties: false,
    },
  };
}

function successEnvelopeSchema(dataSchema: string): JsonObject {
  return {
    type: "object",
    required: ["ok", "data", "meta"],
    properties: {
      ok: { type: "boolean", const: true },
      data: { $ref: `#/components/schemas/${dataSchema}` },
      meta: { $ref: "#/components/schemas/ApiMeta" },
    },
    additionalProperties: false,
  };
}

function pathParameters(pathTemplate: string): JsonObject[] {
  return [...pathTemplate.matchAll(/\{([^}]+)\}/g)].map((match) => ({
    name: match[1],
    in: "path",
    required: true,
    schema: { type: "string", minLength: 1 },
  }));
}

function queryParameter(name: string, required: boolean): JsonObject {
  return {
    name,
    in: "query",
    required,
    schema: { type: "string", ...(required ? { minLength: 1 } : {}) },
  };
}

function securityFor(auth: ApiCapabilityDescriptor["auth"]): JsonObject[] {
  if (auth === "none") return [];
  return [{ [auth === "admin" ? "AdminBearer" : "ChatBearer"]: [] }];
}

function detailedOperation(
  capabilityId: string,
): { operationId: string; responseSchema: string } | undefined {
  return (
    DETAILED_OPERATIONS as Record<
      string,
      { operationId: string; responseSchema: string }
    >
  )[capabilityId];
}

function assertDetailedOperationsExist(): void {
  const capabilityIds = new Set(API_CAPABILITIES.map((entry) => entry.id));
  for (const capabilityId of Object.keys(DETAILED_OPERATIONS)) {
    if (!capabilityIds.has(capabilityId)) {
      throw new Error(
        `detailed OpenAPI operation references missing capability: ${capabilityId}`,
      );
    }
  }
}

function operationIdFrom(capabilityId: string): string {
  const [first = "operation", ...rest] = capabilityId.split(/[._-]+/);
  return [first, ...rest.map(capitalize)].join("");
}

function capitalize(value: string): string {
  return value.length === 0 ? value : value[0]!.toUpperCase() + value.slice(1);
}

function capabilityTags(): string[] {
  return [
    ...new Set(API_CAPABILITIES.flatMap((capability) => [...capability.tags])),
  ].sort();
}

function compareCapabilities(
  left: ApiCapabilityDescriptor,
  right: ApiCapabilityDescriptor,
): number {
  return (
    left.path_template.localeCompare(right.path_template) ||
    left.method.localeCompare(right.method) ||
    left.id.localeCompare(right.id)
  );
}
