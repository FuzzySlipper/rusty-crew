import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainWakeInput } from "./index.js";
import type { ServiceMcpToolCatalog } from "./service-mcp-tools.js";
import {
  type BrainToolResolver,
  resolveToolSession,
} from "./tool-session-selection.js";
import { defaultToolRegistry } from "./tool-registry.js";

export const LIST_AVAILABLE_TOOLS_NAME = "list_available_tools";

const parameters = Type.Object({
  query: Type.Optional(Type.String({ maxLength: 200 })),
  source: Type.Optional(
    Type.Union([Type.Literal("local"), Type.Literal("mcp")]),
  ),
  limit: Type.Optional(Type.Integer({ minimum: 1, maximum: 500 })),
});

type ToolIntrospectionParams = Static<typeof parameters>;

export interface AvailableToolInventoryItem {
  name: string;
  description: string;
  source: "local" | "mcp";
  bindingId?: string;
  serverNames?: string[];
  toolProfileKey?: string;
}

export interface AvailableToolInventoryDetails {
  ok: true;
  sessionId: string;
  profileId: string;
  totalCallable: number;
  totalLocal: number;
  totalMcp: number;
  matched: number;
  returned: number;
  truncated: boolean;
  tools: AvailableToolInventoryItem[];
}

export function createToolIntrospectionResolver(input: {
  baseResolver: BrainToolResolver;
  mcpToolCatalog?: ServiceMcpToolCatalog;
}): BrainToolResolver {
  return ({ wake }) => [
    listAvailableToolsTool({
      wake,
      baseResolver: input.baseResolver,
      mcpToolCatalog: input.mcpToolCatalog,
    }),
  ];
}

function listAvailableToolsTool(input: {
  wake: BrainWakeInput;
  baseResolver: BrainToolResolver;
  mcpToolCatalog?: ServiceMcpToolCatalog;
}): BrainTool<typeof parameters, AvailableToolInventoryDetails> {
  return {
    name: LIST_AVAILABLE_TOOLS_NAME,
    label: "List available tools",
    description:
      "List the tools actually executable for this wake, including dynamic MCP tools and their bindings. Use this instead of guessing tool availability.",
    parameters,
    execute: async (_callId, params) =>
      availableToolInventoryResult(input, params),
  };
}

function availableToolInventoryResult(
  input: {
    wake: BrainWakeInput;
    baseResolver: BrainToolResolver;
    mcpToolCatalog?: ServiceMcpToolCatalog;
  },
  params: ToolIntrospectionParams,
): BrainToolResult<AvailableToolInventoryDetails> {
  const selection = resolveToolSession({
    wake: input.wake,
    resolveTools: input.baseResolver,
  });
  const mcpByName = new Map(
    (
      input.mcpToolCatalog?.candidatesForSession(input.wake.state.session) ?? []
    ).map(({ binding, candidate }) => [candidate.name, binding] as const),
  );
  const introspectionDescription =
    defaultToolRegistry.get(LIST_AVAILABLE_TOOLS_NAME)?.description ??
    "List the tools actually executable for this wake.";
  const tools = [
    ...selection.tools.map<AvailableToolInventoryItem>((tool) => {
      const binding = mcpByName.get(tool.name);
      return binding === undefined
        ? {
            name: tool.name,
            description: tool.description,
            source: "local",
          }
        : {
            name: tool.name,
            description: tool.description,
            source: "mcp",
            bindingId: binding.bindingId,
            serverNames: [...binding.serverNames],
            toolProfileKey: binding.toolProfileKey,
          };
    }),
    {
      name: LIST_AVAILABLE_TOOLS_NAME,
      description: introspectionDescription,
      source: "local" as const,
    },
  ].sort((left, right) => left.name.localeCompare(right.name));
  const query = params.query?.trim().toLowerCase();
  const matched = tools.filter(
    (tool) =>
      (params.source === undefined || tool.source === params.source) &&
      (query === undefined ||
        query.length === 0 ||
        tool.name.toLowerCase().includes(query) ||
        tool.description.toLowerCase().includes(query) ||
        tool.serverNames?.some((server) =>
          server.toLowerCase().includes(query),
        )),
  );
  const limit = params.limit ?? 200;
  const returned = matched.slice(0, limit);
  const totalMcp = tools.filter((tool) => tool.source === "mcp").length;
  const details: AvailableToolInventoryDetails = {
    ok: true,
    sessionId: String(input.wake.sessionId),
    profileId: String(input.wake.state.session.profileId),
    totalCallable: tools.length,
    totalLocal: tools.length - totalMcp,
    totalMcp,
    matched: matched.length,
    returned: returned.length,
    truncated: returned.length < matched.length,
    tools: returned,
  };
  return {
    content: [{ type: "text", text: formatInventory(details) }],
    details,
  };
}

function formatInventory(details: AvailableToolInventoryDetails): string {
  const lines = details.tools.map((tool) => {
    const source =
      tool.source === "mcp"
        ? `mcp:${tool.serverNames?.join(",") || "unknown"}`
        : "local";
    return `- ${tool.name} [${source}] — ${tool.description}`;
  });
  return [
    `Executable tools for session ${details.sessionId}: ${details.totalCallable} total (${details.totalLocal} local, ${details.totalMcp} MCP).`,
    `Matched ${details.matched}; returning ${details.returned}${details.truncated ? " (truncated)" : ""}.`,
    ...lines,
  ].join("\n");
}
