import type {
  AgentTool as PiAgentTool,
  AgentToolResult as PiAgentToolResult,
  AgentToolUpdateCallback as PiAgentToolUpdateCallback,
} from "@earendil-works/pi-agent-core";
import type { Static, TSchema } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainWakeInput } from "./index.js";

export interface PiToolAdapterContext {
  wake: BrainWakeInput;
}

export function toPiAgentTool<TParameters extends TSchema, TDetails = unknown>(
  tool: BrainTool<TParameters, TDetails>,
  context: PiToolAdapterContext,
): PiAgentTool<TParameters, TDetails> {
  return {
    name: tool.name,
    description: tool.description,
    label: tool.label,
    parameters: tool.parameters,
    prepareArguments: tool.prepareArguments,
    execute: async (toolCallId, params, signal, onUpdate) => {
      if (tool.executeWithContext) {
        return toPiToolResult(
          await tool.executeWithContext(params as Static<TParameters>, {
            wake: context.wake,
            wakeId: context.wake.wakeId,
            sessionId: context.wake.sessionId,
            callId: toolCallId,
            signal: signal ?? new AbortController().signal,
            onUpdate: onUpdate
              ? (partial) => onUpdate(toPiToolResult(partial))
              : undefined,
          }),
        );
      }
      return toPiToolResult(
        await tool.execute(
          toolCallId,
          params as Static<TParameters>,
          signal,
          onUpdate ? (partial) => onUpdate(toPiToolResult(partial)) : undefined,
        ),
      );
    },
    executionMode: tool.executionMode,
  };
}

export function toPiAgentTools(
  tools: readonly BrainTool[],
  context: PiToolAdapterContext,
): PiAgentTool[] {
  return tools.map((tool) => toPiAgentTool(tool, context));
}

export function fromPiAgentTool<
  TParameters extends TSchema,
  TDetails = unknown,
>(tool: PiAgentTool<TParameters, TDetails>): BrainTool<TParameters, TDetails> {
  return {
    name: tool.name,
    description: tool.description,
    label: tool.label,
    parameters: tool.parameters,
    prepareArguments: tool.prepareArguments,
    execute: async (toolCallId, params, signal, onUpdate) =>
      fromPiToolResult(
        await tool.execute(
          toolCallId,
          params,
          signal,
          onUpdate
            ? (partial) => onUpdate(fromPiToolResult(partial))
            : undefined,
        ),
      ),
    executionMode: tool.executionMode,
  };
}

export function fromPiAgentTools(tools: readonly PiAgentTool[]): BrainTool[] {
  return tools.map((tool) => fromPiAgentTool(tool));
}

function toPiToolResult<TDetails>(
  result: BrainToolResult<TDetails>,
): PiAgentToolResult<TDetails> {
  const mapped: PiAgentToolResult<TDetails> = {
    content: result.content.map((item) =>
      item.type === "text"
        ? item
        : { type: "image", data: item.data, mimeType: item.mimeType },
    ),
    details: result.details,
  };
  if (result.terminate !== undefined) mapped.terminate = result.terminate;
  return mapped;
}

function fromPiToolResult<TDetails>(
  result: PiAgentToolResult<TDetails>,
): BrainToolResult<TDetails> {
  const mapped: BrainToolResult<TDetails> = {
    content: result.content.map((item) =>
      item.type === "text"
        ? { type: "text", text: item.text }
        : {
            type: "image",
            data: item.data,
            mimeType: item.mimeType,
          },
    ),
    details: result.details,
  };
  if (result.terminate !== undefined) mapped.terminate = result.terminate;
  return mapped;
}

export type LegacyPiAgentToolResolver = (input: {
  wake: BrainWakeInput;
  tools: Parameters<
    import("./tool-session-selection.js").BrainToolResolver
  >[0]["tools"];
  actions?: Parameters<
    import("./tool-session-selection.js").BrainToolResolver
  >[0]["actions"];
}) => PiAgentTool[];

export function adaptLegacyPiAgentToolResolver(
  resolver: LegacyPiAgentToolResolver,
): import("./tool-session-selection.js").BrainToolResolver {
  return (input) => fromPiAgentTools(resolver(input));
}

export function adaptLegacyPiAgentToolResolvers(
  ...resolvers: readonly LegacyPiAgentToolResolver[]
): import("./tool-session-selection.js").BrainToolResolver[] {
  return resolvers.map((resolver) => adaptLegacyPiAgentToolResolver(resolver));
}
