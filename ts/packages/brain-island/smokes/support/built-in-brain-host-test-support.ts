import type { BrainHostExecutor } from "../../src/index.js";
import type { BrainHostContext } from "../../src/brain-host-context.js";
import { createOpenAiResponsesBrainHost } from "../../src/openai-responses-host.js";
import { createPiAgentBrainHost } from "../../src/pi-agent-host.js";

export const fakeOpenAiResponsesBrainHost = {
  createBrain(context: BrainHostContext): Promise<BrainHostExecutor> {
    return createOpenAiResponsesBrainHost(context, { mode: "fake" });
  },
};

export const fakePiAgentBrainHost = {
  async createBrain(context: BrainHostContext): Promise<BrainHostExecutor> {
    return createPiAgentBrainHost(context, { mode: "fake" });
  },
};
