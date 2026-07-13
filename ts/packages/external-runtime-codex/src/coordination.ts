import type { DynamicToolSpec } from "../protocol/0.144.1/ts/v2/DynamicToolSpec.js";

const COORDINATION_NAMESPACE = "rusty_crew";
const MAX_ROUND_TIMEOUT_MS = 300_000;

export const CODEX_COORDINATION_DYNAMIC_TOOLS: readonly DynamicToolSpec[] = [
  {
    type: "namespace",
    name: COORDINATION_NAMESPACE,
    description: "Rusty Crew internal agent coordination.",
    tools: [
      {
        type: "function",
        name: "list_agents",
        description:
          "List agents addressable through this Rusty Crew service, including their stable recipient IDs.",
        inputSchema: {
          type: "object",
          properties: {},
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "send_agent_message",
        description: "Send a message to another Rusty Crew agent.",
        inputSchema: {
          type: "object",
          properties: {
            recipient: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            correlationId: { type: "string", minLength: 1 },
          },
          required: ["recipient", "body"],
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "agent_round",
        description:
          "Send a message to another Rusty Crew agent and wait for one correlated reply.",
        inputSchema: {
          type: "object",
          properties: {
            recipient: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            correlationId: { type: "string", minLength: 1 },
            timeoutMs: {
              type: "integer",
              minimum: 1,
              maximum: MAX_ROUND_TIMEOUT_MS,
            },
          },
          required: ["recipient", "body"],
          additionalProperties: false,
        },
      },
    ],
  },
];
