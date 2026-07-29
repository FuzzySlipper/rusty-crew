import type { BrainHostExecutor } from "./index.js";
import type { BrainModuleSelection } from "./brain-catalog.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { createOpenAiResponsesBrainHost } from "./openai-responses-host.js";
import { createChatCompletionsBrainHost } from "./chat-completions-host.js";

export type { BrainHostContext } from "./brain-host-context.js";

export async function createBuiltInBrainHost(
  selection: BrainModuleSelection,
  context: BrainHostContext,
): Promise<BrainHostExecutor> {
  switch (selection.moduleId) {
    case "chat-completions":
      return createChatCompletionsBrainHost(context);
    case "openai-responses": {
      const strategy = selection.strategy ?? "replay";
      if (strategy !== "replay" && strategy !== "previous-response-chain") {
        throw new Error(`Unsupported OpenAI Responses strategy ${strategy}`);
      }
      return createOpenAiResponsesBrainHost(context, undefined, strategy);
    }
    default:
      throw new Error(
        `Rust selected brain ${selection.moduleId} has no production host executor`,
      );
  }
}
