import type {
  DenMemoryClient,
  DenMemoryClientError,
  DenMemoryRuntimeContext,
  DenMemoryScope,
  DenMemorySourceRef,
} from "@rusty-crew/adapter-den";
import type {
  AgentTool as PiAgentTool,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import type { SessionState } from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type { PiAgentToolResolver } from "./tool-session-selection.js";

export type DenMemoryPolicyMode =
  | "off"
  | "metadata"
  | "candidate"
  | "manual"
  | "permissive";

export interface DenMemoryToolPolicy {
  mode: DenMemoryPolicyMode;
  defaultAudience?: readonly string[];
  defaultRole?: string;
  defaultMemoryMode?: string;
  allowStoreForSessionKinds?: readonly string[];
  allowStoreForProfiles?: readonly string[];
}

export interface DenMemoryToolContext {
  client?: DenMemoryClient;
  policy: DenMemoryToolPolicy;
  runtimeContext?: DenMemoryRuntimeContext;
  session?: Pick<SessionState, "sessionId" | "agentId" | "profileId" | "kind">;
}

export interface DenMemoryToolDetails {
  ok: boolean;
  operation: "recall" | "read" | "search" | "store" | "propose";
  mode: DenMemoryPolicyMode;
  action: "read" | "stored" | "proposed" | "denied" | "failed";
  reasonCode?: string;
  retryable?: boolean;
  result?: unknown;
}

const sourceRefSchema = Type.Object({
  kind: Type.String({ minLength: 1 }),
  ref: Type.String({ minLength: 1 }),
  label: Type.Optional(Type.String()),
});

const scopeSchema = {
  audience: Type.Optional(Type.Array(Type.String({ minLength: 1 }))),
  role: Type.Optional(Type.String({ minLength: 1 })),
  mode: Type.Optional(Type.String({ minLength: 1 })),
  sourceRefs: Type.Optional(Type.Array(sourceRefSchema)),
  metadata: Type.Optional(Type.Record(Type.String(), Type.Unknown())),
};

const recallParameters = Type.Object({
  prompt: Type.String({ minLength: 1 }),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  ...scopeSchema,
});
const readParameters = Type.Object({
  id: Type.Optional(Type.String({ minLength: 1 })),
  slug: Type.Optional(Type.String({ minLength: 1 })),
});
const searchParameters = Type.Object({
  query: Type.String({ minLength: 1 }),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  ...scopeSchema,
});
const storeParameters = Type.Object({
  title: Type.Optional(Type.String({ minLength: 1 })),
  summary: Type.Optional(Type.String({ minLength: 1 })),
  bodyMarkdown: Type.String({ minLength: 1 }),
  ...scopeSchema,
});
const proposeParameters = Type.Object({
  proposalKind: Type.Optional(Type.String({ minLength: 1 })),
  targetMemoryId: Type.Optional(Type.String({ minLength: 1 })),
  title: Type.Optional(Type.String({ minLength: 1 })),
  summary: Type.Optional(Type.String({ minLength: 1 })),
  bodyMarkdown: Type.String({ minLength: 1 }),
  ...scopeSchema,
});

type RecallParams = Static<typeof recallParameters>;
type ReadParams = Static<typeof readParameters>;
type SearchParams = Static<typeof searchParameters>;
type StoreParams = Static<typeof storeParameters>;
type ProposeParams = Static<typeof proposeParameters>;

export function createDenMemoryToolResolver(
  context: DenMemoryToolContext,
): PiAgentToolResolver {
  return () => resolveDenMemoryTools(context);
}

export function resolveDenMemoryTools(
  context: DenMemoryToolContext,
): PiAgentTool[] {
  return [
    denMemoryRecallTool(context),
    denMemoryReadTool(context),
    denMemorySearchTool(context),
    denMemoryStoreTool(context),
    denMemoryProposeTool(context),
  ];
}

export function denMemoryRecallTool(
  context: DenMemoryToolContext,
): PiAgentTool<typeof recallParameters, DenMemoryToolDetails> {
  return {
    name: "den_memory_recall",
    label: "Recall Den memory",
    description:
      "Recall relevant Den-owned memory summaries for the current profile or work context.",
    parameters: recallParameters,
    execute: async (_toolCallId, params: RecallParams) =>
      withMemoryClient(context, "recall", async (client) => {
        const result = await client.recall({
          prompt: params.prompt,
          limit: params.limit,
          ...scope(context, params),
          context: runtimeContext(context),
        });
        return resultDetails("recall", context, "read", result);
      }),
  };
}

export function denMemoryReadTool(
  context: DenMemoryToolContext,
): PiAgentTool<typeof readParameters, DenMemoryToolDetails> {
  return {
    name: "den_memory_read",
    label: "Read Den memory",
    description: "Read a specific Den-owned memory entry by stable reference.",
    parameters: readParameters,
    execute: async (_toolCallId, params: ReadParams) => {
      if (!params.id && !params.slug) {
        return deniedResult("read", context, "missing_memory_ref", false);
      }
      return withMemoryClient(context, "read", async (client) => {
        const result = await client.read({
          id: params.id,
          slug: params.slug,
          context: runtimeContext(context),
        });
        return resultDetails("read", context, "read", result);
      });
    },
  };
}

export function denMemorySearchTool(
  context: DenMemoryToolContext,
): PiAgentTool<typeof searchParameters, DenMemoryToolDetails> {
  return {
    name: "den_memory_search",
    label: "Search Den memory",
    description:
      "Search Den-owned memories through the configured Den Memories service.",
    parameters: searchParameters,
    execute: async (_toolCallId, params: SearchParams) =>
      withMemoryClient(context, "search", async (client) => {
        const result = await client.search({
          query: params.query,
          limit: params.limit,
          ...scope(context, params),
          context: runtimeContext(context),
        });
        return resultDetails("search", context, "read", result);
      }),
  };
}

export function denMemoryStoreTool(
  context: DenMemoryToolContext,
): PiAgentTool<typeof storeParameters, DenMemoryToolDetails> {
  return {
    name: "den_memory_store",
    label: "Store Den memory",
    description:
      "Store a new Den-owned memory or route it to proposal depending on policy.",
    parameters: storeParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: StoreParams) =>
      withMemoryClient(context, "store", async (client) => {
        const mode = context.policy.mode;
        if (mode === "metadata") {
          return deniedResult(
            "store",
            context,
            "den_memory_writes_disabled_metadata_mode",
            false,
          );
        }
        if (mode === "manual") {
          return deniedResult(
            "store",
            context,
            "den_memory_manual_review_required",
            false,
          );
        }
        if (mode === "candidate" || !canStoreDirectly(context)) {
          const result = await client.propose({
            proposalKind: "store",
            title: params.title,
            summary: params.summary,
            bodyMarkdown: params.bodyMarkdown,
            ...scope(context, params),
            context: runtimeContext(context),
          });
          return resultDetails("store", context, "proposed", result);
        }
        const result = await client.store({
          title: params.title,
          summary: params.summary,
          bodyMarkdown: params.bodyMarkdown,
          ...scope(context, params),
          context: runtimeContext(context),
        });
        return resultDetails("store", context, "stored", result);
      }),
  };
}

export function denMemoryProposeTool(
  context: DenMemoryToolContext,
): PiAgentTool<typeof proposeParameters, DenMemoryToolDetails> {
  return {
    name: "den_memory_propose",
    label: "Propose Den memory",
    description:
      "Propose a Den-owned memory change for review without direct storage.",
    parameters: proposeParameters,
    executionMode: "sequential",
    execute: async (_toolCallId, params: ProposeParams) =>
      withMemoryClient(context, "propose", async (client) => {
        if (context.policy.mode === "metadata") {
          return deniedResult(
            "propose",
            context,
            "den_memory_proposals_disabled_metadata_mode",
            false,
          );
        }
        const result = await client.propose({
          proposalKind: params.proposalKind ?? "store",
          targetMemoryId: params.targetMemoryId,
          title: params.title,
          summary: params.summary,
          bodyMarkdown: params.bodyMarkdown,
          ...scope(context, params),
          context: runtimeContext(context),
        });
        return resultDetails("propose", context, "proposed", result);
      }),
  };
}

async function withMemoryClient(
  context: DenMemoryToolContext,
  operation: DenMemoryToolDetails["operation"],
  callback: (
    client: DenMemoryClient,
  ) => Promise<AgentToolResult<DenMemoryToolDetails>>,
): Promise<AgentToolResult<DenMemoryToolDetails>> {
  if (context.policy.mode === "off") {
    return deniedResult(operation, context, "den_memory_policy_off", false);
  }
  if (!context.client) {
    return deniedResult(
      operation,
      context,
      "den_memory_client_unavailable",
      true,
    );
  }
  try {
    return await callback(context.client);
  } catch (error) {
    return errorResult(operation, context, error);
  }
}

function scope(
  context: DenMemoryToolContext,
  params: {
    audience?: readonly string[];
    role?: string;
    mode?: string;
    sourceRefs?: readonly DenMemorySourceRef[];
    metadata?: Record<string, unknown>;
  },
): DenMemoryScope & {
  sourceRefs?: readonly DenMemorySourceRef[];
  metadata?: Record<string, unknown>;
} {
  return {
    audience: params.audience ?? context.policy.defaultAudience,
    role: params.role ?? context.policy.defaultRole,
    mode: params.mode ?? context.policy.defaultMemoryMode,
    sourceRefs: params.sourceRefs,
    metadata: params.metadata,
  };
}

function runtimeContext(
  context: DenMemoryToolContext,
): DenMemoryRuntimeContext {
  return {
    ...context.runtimeContext,
    sessionId: context.runtimeContext?.sessionId ?? context.session?.sessionId,
    agentId: context.runtimeContext?.agentId ?? context.session?.agentId,
    profileId: context.runtimeContext?.profileId ?? context.session?.profileId,
  };
}

function canStoreDirectly(context: DenMemoryToolContext): boolean {
  if (context.policy.mode !== "permissive") {
    return false;
  }
  const sessionKind = context.session?.kind;
  const profileId = context.session?.profileId;
  const allowedKinds = context.policy.allowStoreForSessionKinds ?? ["full"];
  const allowedProfiles = context.policy.allowStoreForProfiles ?? ["prime"];
  return (
    (sessionKind !== undefined && allowedKinds.includes(sessionKind)) ||
    (profileId !== undefined && allowedProfiles.includes(profileId))
  );
}

function resultDetails(
  operation: DenMemoryToolDetails["operation"],
  context: DenMemoryToolContext,
  action: DenMemoryToolDetails["action"],
  result: unknown,
): AgentToolResult<DenMemoryToolDetails> {
  const details = {
    ok: true,
    operation,
    mode: context.policy.mode,
    action,
    result,
  } satisfies DenMemoryToolDetails;
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function deniedResult(
  operation: DenMemoryToolDetails["operation"],
  context: DenMemoryToolContext,
  reasonCode: string,
  retryable: boolean,
): AgentToolResult<DenMemoryToolDetails> {
  const details = {
    ok: false,
    operation,
    mode: context.policy.mode,
    action: "denied",
    reasonCode,
    retryable,
  } satisfies DenMemoryToolDetails;
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function errorResult(
  operation: DenMemoryToolDetails["operation"],
  context: DenMemoryToolContext,
  error: unknown,
): AgentToolResult<DenMemoryToolDetails> {
  const memoryError = error as Partial<DenMemoryClientError>;
  const details = {
    ok: false,
    operation,
    mode: context.policy.mode,
    action: "failed",
    reasonCode:
      memoryError.options?.reasonCode ??
      memoryError.code ??
      "den_memory_request_failed",
    retryable: memoryError.options?.retryable ?? true,
    result: {
      message:
        error instanceof Error ? error.message : "Den Memories request failed",
    },
  } satisfies DenMemoryToolDetails;
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}
