import type { AgentTool as PiAgentTool } from "@earendil-works/pi-agent-core";
import type { ToolDescriptor, ToolProfile } from "@rusty-crew/contracts";
import type { BrainWakeInput } from "./index.js";

export type PiAgentToolResolver = (input: {
  wake: BrainWakeInput;
  tools: ToolDescriptor[];
}) => PiAgentTool[];

export type ToolSessionSelectionStatus =
  | "callable"
  | "implementation_missing"
  | "duplicate_implementation"
  | "not_requested";

export interface ToolSessionSelectionItem {
  name: string;
  descriptor?: ToolDescriptor;
  tool?: PiAgentTool;
  status: ToolSessionSelectionStatus;
  reasons: string[];
}

export interface ToolSessionSelectionInput {
  wake: BrainWakeInput;
  toolProfile?: ToolProfile;
  resolveTools?: PiAgentToolResolver;
}

export interface ToolSessionSelection {
  tools: PiAgentTool[];
  items: ToolSessionSelectionItem[];
}

export function resolveToolSession(
  input: ToolSessionSelectionInput,
): ToolSessionSelection {
  const descriptors =
    input.toolProfile?.tools ?? input.wake.state.session.toolProfile.tools;
  const descriptorsByName = new Map(
    descriptors.map((descriptor) => [descriptor.name, descriptor]),
  );
  const requestedNames = new Set(descriptorsByName.keys());
  const implementationsByName = groupToolsByName(
    input.resolveTools?.({ wake: input.wake, tools: descriptors }) ?? [],
  );

  const descriptorItems = descriptors.map<ToolSessionSelectionItem>(
    (descriptor) => {
      const tools = implementationsByName.get(descriptor.name) ?? [];
      if (tools.length === 0) {
        return {
          name: descriptor.name,
          descriptor,
          status: "implementation_missing",
          reasons: [
            `${descriptor.name} has no resolved Pi tool implementation`,
          ],
        };
      }
      if (tools.length > 1) {
        return {
          name: descriptor.name,
          descriptor,
          status: "duplicate_implementation",
          reasons: [
            `${descriptor.name} resolved to ${tools.length} Pi tool implementations`,
          ],
        };
      }
      const [tool] = tools;
      return {
        name: descriptor.name,
        descriptor,
        tool,
        status: "callable",
        reasons: [`${descriptor.name} is allowed by the session ToolProfile`],
      };
    },
  );

  const unexpectedItems = [...implementationsByName.entries()]
    .filter(([name]) => !requestedNames.has(name))
    .sort(([left], [right]) => left.localeCompare(right))
    .flatMap(([name, tools]) =>
      tools.map<ToolSessionSelectionItem>((tool) => ({
        name,
        tool,
        status: "not_requested",
        reasons: [`${name} was resolved but is not present in ToolProfile`],
      })),
    );

  return {
    tools: descriptorItems.flatMap((item) =>
      item.status === "callable" && item.tool ? [item.tool] : [],
    ),
    items: [...descriptorItems, ...unexpectedItems],
  };
}

function groupToolsByName(
  tools: readonly PiAgentTool[],
): Map<string, PiAgentTool[]> {
  const byName = new Map<string, PiAgentTool[]>();
  for (const tool of tools) {
    const group = byName.get(tool.name);
    if (group) {
      group.push(tool);
    } else {
      byName.set(tool.name, [tool]);
    }
  }
  return byName;
}
