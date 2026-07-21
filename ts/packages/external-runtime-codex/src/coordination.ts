import type { DynamicToolSpec } from "../protocol/0.144.1/ts/v2/DynamicToolSpec.js";

const COORDINATION_NAMESPACE = "rusty_crew";
const MAX_ROUND_TIMEOUT_MS = 300_000;
const MAX_MESSAGE_TTL_SECONDS = 86_400;

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
          "List curated Rusty Crew @route addresses first, followed by raw agent diagnostics.",
        inputSchema: {
          type: "object",
          properties: {},
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "send_agent_message",
        description:
          "Send a message to an exact Rusty Crew @route address or raw agent ID.",
        inputSchema: {
          type: "object",
          properties: {
            recipient: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            correlationId: { type: "string", minLength: 1 },
            ttlSeconds: {
              type: "integer",
              minimum: 1,
              maximum: MAX_MESSAGE_TTL_SECONDS,
            },
          },
          required: ["recipient", "body"],
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "reply_agent_message",
        description:
          "Reply once to a routed Rusty Crew message. Crew resolves its sender and correlation from the message ID.",
        inputSchema: {
          type: "object",
          properties: {
            messageId: { type: "string", minLength: 1 },
            body: { type: "string", minLength: 1 },
            ttlSeconds: {
              type: "integer",
              minimum: 1,
              maximum: MAX_MESSAGE_TTL_SECONDS,
            },
          },
          required: ["messageId", "body"],
          additionalProperties: false,
        },
      },
      {
        type: "function",
        name: "agent_round",
        description:
          "Send a message to an exact Rusty Crew @route address or raw agent ID and wait for one correlated reply.",
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
