import type { BrainHostExecutor } from "./index.js";
import type { BrainModuleSelection } from "./brain-catalog.js";
import type { BrainHostContext } from "./brain-host-context.js";
import { createOpenAiResponsesBrainHost } from "./openai-responses-host.js";
import { createPiAgentBrainHost } from "./pi-agent-host.js";

export type { BrainHostContext } from "./brain-host-context.js";

export async function createBuiltInBrainHost(
  selection: BrainModuleSelection,
  context: BrainHostContext,
): Promise<BrainHostExecutor> {
  switch (selection.moduleId) {
    case "pi-agent":
      return createPiAgentBrainHost(context);
    case "openai-responses":
      return createOpenAiResponsesBrainHost(context);
    default:
      throw new Error(
        `Rust selected brain ${selection.moduleId} has no production host executor`,
      );
  }
}
