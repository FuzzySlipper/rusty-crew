import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

const messageParameters = Type.Object({
  recipientAlias: Type.String({ minLength: 1 }),
  body: Type.String({ minLength: 1 }),
  correlationId: Type.Optional(Type.String({ minLength: 1 })),
  replyToMessageId: Type.Optional(Type.String({ minLength: 1 })),
});
const directoryParameters = Type.Object({}, { additionalProperties: false });
type MessageParams = Static<typeof messageParameters>;

export interface CrewServicesToolRuntime {
  available(sessionId: string): boolean;
  directory(sessionId: string): Promise<readonly { alias: string; routeRevision: number }[]>;
  message(input: {
    sessionId: string;
    toolCallId: string;
    recipientAlias: string;
    body: string;
    correlationId?: string;
    replyToMessageId?: string;
  }): Promise<{ messageId: string; replayed: boolean }>;
}

export function createCrewServicesToolResolver(
  runtime?: CrewServicesToolRuntime,
): BrainToolResolver {
  return ({ wake }) =>
    runtime === undefined || !runtime.available(wake.state.session.sessionId)
      ? []
      : [directoryTool(runtime), messageTool(runtime)];
}

export function directoryTool(runtime?: CrewServicesToolRuntime): BrainTool {
  return {
    name: "crew_directory",
    label: "Crew directory",
    description: "List exact bound crew aliases without exposing session identifiers.",
    parameters: directoryParameters,
    executeWithContext: async (_params, context) => {
      if (runtime === undefined) return unavailable("crew_directory");
      try {
        const agents = await runtime.directory(context.sessionId);
        return result("crew_directory", true, { agents });
      } catch (error) {
        return result("crew_directory", false, { reasonCode: "crew_directory_rejected", error: message(error) });
      }
    },
    execute: async () => result("crew_directory", false, { reasonCode: "tool_context_required" }),
  };
}

export function messageTool(runtime?: CrewServicesToolRuntime): BrainTool<typeof messageParameters> {
  return {
    name: "crew_message",
    label: "Crew message",
    description: "Send one ordinary fabric message to a bound crew alias; replies are terminal by default.",
    parameters: messageParameters,
    executeWithContext: async (params: MessageParams, context) => {
      if (runtime === undefined) return unavailable("crew_message");
      try {
        const receipt = await runtime.message({
          sessionId: context.sessionId,
          toolCallId: context.callId,
          recipientAlias: params.recipientAlias,
          body: params.body,
          ...(params.correlationId === undefined ? {} : { correlationId: params.correlationId }),
          ...(params.replyToMessageId === undefined ? {} : { replyToMessageId: params.replyToMessageId }),
        });
        return result("crew_message", true, receipt);
      } catch (error) {
        return result("crew_message", false, { reasonCode: "crew_message_rejected", error: message(error) });
      }
    },
    execute: async () => result("crew_message", false, { reasonCode: "tool_context_required" }),
  };
}

function unavailable(operation: string): BrainToolResult {
  return result(operation, false, { reasonCode: "crew_services_unavailable" });
}
function result(operation: string, ok: boolean, details: Record<string, unknown>): BrainToolResult {
  const value = { ok, operation, ...details };
  return { content: [{ type: "text", text: JSON.stringify(value) }], details: value };
}
function message(error: unknown): string { return error instanceof Error ? error.message : "crew-services operation failed"; }
