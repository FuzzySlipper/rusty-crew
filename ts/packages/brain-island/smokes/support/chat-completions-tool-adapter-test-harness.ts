import type {
  AgentTool as ChatCompletionsTool,
  AgentToolResult as ChatCompletionsToolResult,
  AgentToolUpdateCallback as ChatCompletionsToolUpdateCallback,
} from "./chat-completions-test-harness.js";
import type { Static, TSchema } from "typebox";
import type { BrainTool, BrainToolResult } from "../../src/brain-tool.js";
import type { BrainWakeInput } from "../../src/index.js";
import {
  localToolCallMetadata,
  type ToolCallDebugStore,
} from "../../src/tool-call-debug-store.js";

export interface ChatCompletionsToolAdapterContext {
  wake: BrainWakeInput;
  toolCallDebugStore?: ToolCallDebugStore;
}

export function toChatCompletionsTool<
  TParameters extends TSchema,
  TDetails = unknown,
>(
  tool: BrainTool<TParameters, TDetails>,
  context: ChatCompletionsToolAdapterContext,
): ChatCompletionsTool<TParameters, TDetails> {
  return {
    name: tool.name,
    description: tool.description,
    label: tool.label,
    parameters: tool.parameters,
    prepareArguments: tool.prepareArguments,
    execute: async (toolCallId, params, signal, onUpdate) => {
      const debugRecord = context.toolCallDebugStore?.start({
        toolCallId,
        sessionId: context.wake.sessionId,
        wakeId: context.wake.wakeId,
        toolName: tool.name,
        arguments: params,
        sourceMetadata: localToolCallMetadata(tool.name),
      });
      const recordUpdate = <TDetails>(
        partial: BrainToolResult<TDetails>,
      ): BrainToolResult<TDetails> => {
        if (debugRecord) {
          context.toolCallDebugStore?.recordUpdate({
            debugDetailId: debugRecord.debug_detail_id,
            partialResult: partial,
          });
        }
        return partial;
      };
      if (tool.executeWithContext) {
        try {
          const result = await tool.executeWithContext(
            params as Static<TParameters>,
            {
              wake: context.wake,
              wakeId: context.wake.wakeId,
              sessionId: context.wake.sessionId,
              callId: toolCallId,
              signal: signal ?? new AbortController().signal,
              onUpdate: onUpdate
                ? (partial) =>
                    onUpdate(toChatCompletionsToolResult(recordUpdate(partial)))
                : undefined,
            },
          );
          if (debugRecord) {
            context.toolCallDebugStore?.finish({
              debugDetailId: debugRecord.debug_detail_id,
              finalResult: result,
            });
          }
          return toChatCompletionsToolResult(result);
        } catch (error) {
          if (debugRecord) {
            context.toolCallDebugStore?.fail({
              debugDetailId: debugRecord.debug_detail_id,
              error,
            });
          }
          throw error;
        }
      }
      try {
        const result = await tool.execute(
          toolCallId,
          params as Static<TParameters>,
          signal,
          onUpdate
            ? (partial) =>
                onUpdate(toChatCompletionsToolResult(recordUpdate(partial)))
            : undefined,
        );
        if (debugRecord) {
          context.toolCallDebugStore?.finish({
            debugDetailId: debugRecord.debug_detail_id,
            finalResult: result,
          });
        }
        return toChatCompletionsToolResult(result);
      } catch (error) {
        if (debugRecord) {
          context.toolCallDebugStore?.fail({
            debugDetailId: debugRecord.debug_detail_id,
            error,
          });
        }
        throw error;
      }
    },
    executionMode: tool.executionMode,
  };
}

export function toChatCompletionsTools(
  tools: readonly BrainTool[],
  context: ChatCompletionsToolAdapterContext,
): ChatCompletionsTool[] {
  return tools.map((tool) => toChatCompletionsTool(tool, context));
}

export function fromChatCompletionsTool<
  TParameters extends TSchema,
  TDetails = unknown,
>(
  tool: ChatCompletionsTool<TParameters, TDetails>,
): BrainTool<TParameters, TDetails> {
  return {
    name: tool.name,
    description: tool.description,
    label: tool.label,
    parameters: tool.parameters,
    prepareArguments: tool.prepareArguments,
    execute: async (toolCallId, params, signal, onUpdate) =>
      fromChatCompletionsToolResult(
        await tool.execute(
          toolCallId,
          params,
          signal,
          onUpdate
            ? (partial) => onUpdate(fromChatCompletionsToolResult(partial))
            : undefined,
        ),
      ),
    executionMode: tool.executionMode,
  };
}

export function fromChatCompletionsTools(
  tools: readonly ChatCompletionsTool[],
): BrainTool[] {
  return tools.map((tool) => fromChatCompletionsTool(tool));
}

function toChatCompletionsToolResult<TDetails>(
  result: BrainToolResult<TDetails>,
): ChatCompletionsToolResult<TDetails> {
  const mapped: ChatCompletionsToolResult<TDetails> = {
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

function fromChatCompletionsToolResult<TDetails>(
  result: ChatCompletionsToolResult<TDetails>,
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

export type LegacyChatCompletionsToolResolver = (input: {
  wake: BrainWakeInput;
  tools: Parameters<
    import("../../src/tool-session-selection.js").BrainToolResolver
  >[0]["tools"];
  actions?: Parameters<
    import("../../src/tool-session-selection.js").BrainToolResolver
  >[0]["actions"];
}) => ChatCompletionsTool[];

export function adaptLegacyChatCompletionsToolResolver(
  resolver: LegacyChatCompletionsToolResolver,
): import("../../src/tool-session-selection.js").BrainToolResolver {
  return (input) => fromChatCompletionsTools(resolver(input));
}

export function adaptLegacyChatCompletionsToolResolvers(
  ...resolvers: readonly LegacyChatCompletionsToolResolver[]
): import("../../src/tool-session-selection.js").BrainToolResolver[] {
  return resolvers.map((resolver) =>
    adaptLegacyChatCompletionsToolResolver(resolver),
  );
}
