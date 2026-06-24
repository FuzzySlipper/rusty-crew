import type { McpBindingRecord } from "@rusty-crew/contracts";
import { Type, type TSchema } from "typebox";

export interface BrainCompatibleToolResult<TDetails = unknown> {
  content: Array<{ type: "text"; text: string }>;
  details: TDetails;
  terminate?: boolean;
}

export interface BrainCompatibleTool<
  TParameters extends TSchema = TSchema,
  TDetails = unknown,
> {
  name: string;
  description: string;
  label: string;
  parameters: TParameters;
  execute(
    toolCallId: string,
    params: unknown,
    signal?: AbortSignal,
  ): Promise<BrainCompatibleToolResult<TDetails>>;
  executionMode?: "sequential" | "parallel";
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
    | Array<{ type: "text"; text: string } | { type: "image"; image: unknown }>;
  details?: unknown;
  isError?: boolean;
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
  parameters: TSchema;
  outputSchema?: JsonSchemaValue;
  annotations: Record<string, unknown>;
}

export interface McpDiscoveryReport {
  bindingId: string;
  toolProfileKey: string;
  discoveredToolRevision?: string;
  candidates: McpRegistryCandidate[];
  issues: McpDiscoveryIssue[];
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

export async function discoverMcpToolCandidates(
  binding: McpBindingRecord,
  client: McpToolDiscoveryClient,
): Promise<McpDiscoveryReport> {
  const tools = await client.listTools();
  return convertMcpToolsToCandidates(binding, tools);
}

export function convertMcpToolsToCandidates(
  binding: McpBindingRecord,
  tools: readonly McpDiscoveredTool[],
): McpDiscoveryReport {
  const issues: McpDiscoveryIssue[] = [];
  const seenSourceNames = new Set<string>();
  const candidates = tools.map((tool) => {
    if (seenSourceNames.has(tool.name)) {
      issues.push({
        severity: "error",
        code: "duplicate_source_tool",
        toolName: tool.name,
        message: `MCP server list returned duplicate source tool ${tool.name}`,
      });
    }
    seenSourceNames.add(tool.name);

    const normalizedName = mcpModelToolName(binding, tool.name);
    if (!isValidModelToolName(normalizedName)) {
      issues.push({
        severity: "error",
        code: "invalid_name",
        toolName: tool.name,
        message: `MCP tool ${tool.name} cannot be normalized to a model-callable name`,
      });
    }

    const schema = normalizeMcpInputSchema(tool.inputSchema, tool.name, issues);
    return {
      name: normalizedName,
      description:
        tool.description ??
        tool.title ??
        `MCP tool ${tool.name} from ${binding.serverNames.join(", ")}`,
      category: "mcp" as const,
      toolsets: [`mcp:${binding.toolProfileKey}`],
      implementationModule: "@rusty-crew/adapter-mcp#mcpToolExecutor",
      surfaces: ["brain", "mcp"] as const,
      safety: mcpSafetyFlags(tool),
      outputShape: outputShapeForTool(binding, tool),
      version: "1.0.0",
      inventoryTest: "smoke:mcp",
      coexistenceNote: "MCP source identity disambiguates imported tools.",
      source: {
        bindingId: binding.bindingId,
        adapterId: binding.adapterId,
        serverNames: [...binding.serverNames],
        sourceToolName: tool.name,
        catalogRevision: binding.discoveredToolRevision,
        endpointRef: binding.endpointRef,
      },
      parameters: schema,
      outputSchema: tool.outputSchema,
      annotations: tool.annotations ?? {},
    } satisfies McpRegistryCandidate;
  });

  return {
    bindingId: binding.bindingId,
    toolProfileKey: binding.toolProfileKey,
    discoveredToolRevision: binding.discoveredToolRevision,
    candidates,
    issues,
  };
}

export function createMcpBrainTool(
  binding: McpBindingRecord,
  candidate: McpRegistryCandidate,
  executor: McpToolExecutor,
): BrainCompatibleTool<TSchema, McpToolExecutionResult["details"]> {
  return {
    name: candidate.name,
    description: candidate.description,
    label: candidate.annotations.title
      ? String(candidate.annotations.title)
      : candidate.source.sourceToolName,
    parameters: candidate.parameters,
    executionMode: "sequential",
    execute: async (toolCallId, params, signal) => {
      const result = await executor.callTool({
        binding,
        toolName: candidate.source.sourceToolName,
        arguments: params,
        toolCallId,
        signal,
      });
      return toBrainToolResult(result);
    },
  };
}

export function normalizeMcpInputSchema(
  schema: JsonSchemaValue | undefined,
  toolName = "unknown",
  issues: McpDiscoveryIssue[] = [],
): TSchema {
  if (schema === undefined || schema === true) {
    return Type.Object({});
  }
  if (schema === false) {
    issues.push({
      severity: "warning",
      code: "schema_sanitized",
      toolName,
      message: "false MCP input schema was replaced with an empty object",
    });
    return Type.Object({});
  }

  const normalized = sanitizeJsonSchema(schema, toolName, issues);
  if (!isObjectSchema(normalized)) {
    issues.push({
      severity: "warning",
      code: "schema_wrapped",
      toolName,
      message: "non-object MCP input schema was wrapped as an object value",
    });
    return Type.Unsafe({
      type: "object",
      properties: { value: normalized },
      required: ["value"],
      additionalProperties: false,
    });
  }

  return Type.Unsafe(normalized);
}

function sanitizeJsonSchema(
  schema: JsonSchemaValue,
  toolName: string,
  issues: McpDiscoveryIssue[],
): Record<string, unknown> {
  if (schema === true) return {};
  if (schema === false) {
    issues.push({
      severity: "warning",
      code: "schema_sanitized",
      toolName,
      message: "nested false schema was replaced with an empty schema",
    });
    return {};
  }

  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(schema)) {
    if (value === undefined || key === "$schema" || key === "$id") continue;
    if (key === "properties" && isRecord(value)) {
      result.properties = Object.fromEntries(
        Object.entries(value).map(([propertyName, propertySchema]) => [
          propertyName,
          sanitizeJsonSchema(
            propertySchema as JsonSchemaValue,
            toolName,
            issues,
          ),
        ]),
      );
      continue;
    }
    if (key === "items") {
      result.items = sanitizeJsonSchema(
        value as JsonSchemaValue,
        toolName,
        issues,
      );
      continue;
    }
    if (key === "additionalProperties" && typeof value !== "boolean") {
      result.additionalProperties = sanitizeJsonSchema(
        value as JsonSchemaValue,
        toolName,
        issues,
      );
      continue;
    }
    if (
      (key === "anyOf" || key === "oneOf" || key === "allOf") &&
      Array.isArray(value)
    ) {
      result[key] = value.map((item) =>
        sanitizeJsonSchema(item as JsonSchemaValue, toolName, issues),
      );
      continue;
    }
    if ((key === "$defs" || key === "definitions") && isRecord(value)) {
      result[key] = Object.fromEntries(
        Object.entries(value).map(([definitionName, definitionSchema]) => [
          definitionName,
          sanitizeJsonSchema(
            definitionSchema as JsonSchemaValue,
            toolName,
            issues,
          ),
        ]),
      );
      continue;
    }
    result[key] = value;
  }

  if (Array.isArray(result.type) && result.type.includes("null")) {
    const nonNull = result.type.filter((value) => value !== "null");
    result.type = nonNull.length === 1 ? nonNull[0] : nonNull;
    result.nullable = true;
  }

  return result;
}

function mcpModelToolName(
  binding: McpBindingRecord,
  sourceName: string,
): string {
  const normalized = sourceName
    .trim()
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .replace(/[^A-Za-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "")
    .toLowerCase();
  const serverPrefix =
    binding.serverNames.length === 1
      ? binding.serverNames[0]!.trim()
          .replace(/[^A-Za-z0-9]+/g, "_")
          .replace(/^_+|_+$/g, "")
          .toLowerCase()
      : "mcp";
  return `${serverPrefix}_${normalized}`;
}

function isValidModelToolName(name: string): boolean {
  return /^[a-z][a-z0-9_]*$/.test(name);
}

function isObjectSchema(schema: Record<string, unknown>): boolean {
  return schema.type === "object" || schema.properties !== undefined;
}

function mcpSafetyFlags(
  tool: McpDiscoveredTool,
): readonly ("network_access" | "external_write")[] {
  const destructive = tool.annotations?.destructiveHint === true;
  return destructive
    ? (["network_access", "external_write"] as const)
    : (["network_access"] as const);
}

function outputShapeForTool(
  binding: McpBindingRecord,
  tool: McpDiscoveredTool,
): string {
  const server =
    binding.serverNames.length === 1
      ? binding.serverNames[0]!.replace(/[^A-Za-z0-9]+/g, "_").toLowerCase()
      : "multi_server";
  return `mcp.${server}.${tool.name.replace(/[^A-Za-z0-9]+/g, "_").toLowerCase()}.result.v1`;
}

function toBrainToolResult(
  result: McpToolExecutionResult,
): BrainCompatibleToolResult<McpToolExecutionResult["details"]> {
  return {
    content:
      typeof result.content === "string"
        ? [{ type: "text", text: result.content }]
        : result.content.map((item) =>
            item.type === "text"
              ? item
              : {
                  type: "text",
                  text: "[image content returned by MCP tool]",
                },
          ),
    details: result.details,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
