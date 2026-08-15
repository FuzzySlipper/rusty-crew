import assert from "node:assert/strict";
import test from "node:test";
import { Type } from "typebox";

import type { BrainTool } from "../src/brain-tool.js";
import { defaultBodyDeltaPolicy } from "../src/index.js";
import {
  createToolIntrospectionResolver,
  type AvailableToolInventoryDetails,
} from "../src/tool-introspection.js";
import type { BrainToolResolver } from "../src/tool-session-selection.js";

test("reports the executable wake selection and identifies MCP bindings", async () => {
  const wake = {
    wakeId: "wake-tools",
    sessionId: "session-tools",
    systemPrompt: "system",
    roleAssembly: { instructions: "inspect tools truthfully" },
    state: {
      session: {
        handle: 1,
        sessionId: "session-tools",
        agentId: "engineer",
        profileId: "rusty-engineer",
        kind: "full",
        resourceLimits: {},
        toolProfile: {
          tools: [
            { name: "read_file", description: "Read files" },
            { name: "den_list_tasks", description: "List Den tasks" },
            {
              name: "list_available_tools",
              description: "List executable tools",
            },
            { name: "missing_tool", description: "Missing implementation" },
          ],
        },
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-08-15T00:00:00.000Z",
        lastActiveAt: "2026-08-15T00:00:00.000Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    },
  } as never;
  const baseResolver: BrainToolResolver = () => [
    fakeTool("read_file"),
    fakeTool("den_list_tasks"),
  ];
  const resolver = createToolIntrospectionResolver({
    baseResolver,
    mcpToolCatalog: {
      candidatesForSession() {
        return [
          {
            binding: {
              bindingId: "engineer-den--session--session-tools",
              adapterId: "mcp-ts-main",
              agentId: "engineer",
              sessionId: "session-tools",
              profileId: "rusty-engineer",
              serverNames: ["den"],
              endpointRef: "config://mcp/den",
              transport: "streamable_http",
              toolProfileKey: "rusty-engineer",
              status: "active",
              diagnostics: {},
            },
            candidate: { name: "den_list_tasks" },
          },
        ] as never;
      },
      registryForProfile() {
        return undefined;
      },
      toolsetsForProfile() {
        return [];
      },
      reports: [],
    },
  });
  const [tool] = resolver({ wake, tools: [] });
  assert.ok(tool);

  const result = await tool.execute("call-1", {
    query: "task",
    source: "mcp",
  });
  const details = result.details as AvailableToolInventoryDetails;

  assert.equal(details.totalCallable, 3);
  assert.equal(details.totalLocal, 2);
  assert.equal(details.totalMcp, 1);
  assert.deepEqual(details.tools, [
    {
      name: "den_list_tasks",
      description: "den_list_tasks description",
      source: "mcp",
      bindingId: "engineer-den--session--session-tools",
      serverNames: ["den"],
      toolProfileKey: "rusty-engineer",
    },
  ]);
  assert.doesNotMatch(
    result.content[0]?.type === "text" ? result.content[0].text : "",
    /missing_tool/,
  );
});

function fakeTool(name: string): BrainTool {
  return {
    name,
    description: `${name} description`,
    label: name,
    parameters: Type.Object({}),
    execute: async () => ({
      content: [{ type: "text", text: `${name} result` }],
      details: {},
    }),
  };
}
