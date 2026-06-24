import type {
  BrainAction,
  ToolDescriptor,
  ToolProfile,
} from "@rusty-crew/contracts";
import type { BrainTool } from "./brain-tool.js";
import type { BrainWakeInput } from "./index.js";

export interface BrainActionCollector {
  add(action: BrainAction): void;
  addMany(actions: readonly BrainAction[]): void;
  readonly actions: readonly BrainAction[];
}

export type BrainToolResolver = (input: {
  wake: BrainWakeInput;
  tools: ToolDescriptor[];
  actions?: BrainActionCollector;
}) => BrainTool[];

export type ToolSessionSelectionStatus =
  | "callable"
  | "implementation_missing"
  | "duplicate_implementation"
  | "not_requested";

export interface ToolSessionSelectionItem {
  name: string;
  descriptor?: ToolDescriptor;
  tool?: BrainTool;
  status: ToolSessionSelectionStatus;
  reasons: string[];
}

export interface ToolSessionSelectionInput {
  wake: BrainWakeInput;
  toolProfile?: ToolProfile;
  resolveTools?: BrainToolResolver;
  actions?: BrainActionCollector;
}

export interface ToolSessionSelection {
  tools: BrainTool[];
  items: ToolSessionSelectionItem[];
}

/**
 * Compose resolver implementations only. ToolProfile filtering and duplicate
 * detection remain centralized in resolveToolSession.
 */
export function combineResolvers(
  ...resolvers: readonly BrainToolResolver[]
): BrainToolResolver {
  return (input) => resolvers.flatMap((resolver) => resolver(input));
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
    input.resolveTools?.({
      wake: input.wake,
      tools: descriptors,
      actions: input.actions,
    }) ?? [],
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
            `${descriptor.name} has no resolved brain tool implementation`,
          ],
        };
      }
      if (tools.length > 1) {
        return {
          name: descriptor.name,
          descriptor,
          status: "duplicate_implementation",
          reasons: [
            `${descriptor.name} resolved to ${tools.length} brain tool implementations`,
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
  tools: readonly BrainTool[],
): Map<string, BrainTool[]> {
  const byName = new Map<string, BrainTool[]>();
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
