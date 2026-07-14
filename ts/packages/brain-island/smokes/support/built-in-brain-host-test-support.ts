import type { BrainHostExecutor } from "../../src/index.js";
import type { BrainHostContext } from "../../src/brain-host-context.js";
import { createOpenAiResponsesBrainHost } from "../../src/openai-responses-host.js";
import { createChatCompletionsBrainHost } from "../../src/chat-completions-host.js";

export const fakeOpenAiResponsesBrainHost = {
  createBrain(context: BrainHostContext): Promise<BrainHostExecutor> {
    return createOpenAiResponsesBrainHost(context, { mode: "fake" });
  },
};

export const fakeChatCompletionsBrainHost = {
  async createBrain(context: BrainHostContext): Promise<BrainHostExecutor> {
    return createChatCompletionsBrainHost(context, { mode: "fake" });
  },
};
