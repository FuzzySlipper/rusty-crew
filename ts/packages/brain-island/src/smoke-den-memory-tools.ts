import assert from "node:assert/strict";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import type {
  DenMemoryClient,
  DenMemoryProposeRequest,
  DenMemoryStoreRequest,
} from "@rusty-crew/adapter-den";
import {
  denMemoryProposeTool,
  denMemoryReadTool,
  denMemoryRecallTool,
  denMemorySearchTool,
  denMemoryStoreTool,
  resolveDenMemoryTools,
} from "./index.js";
import type { DenMemoryToolContext, DenMemoryToolDetails } from "./index.js";

const calls: Array<{ method: string; payload: unknown }> = [];
const client: DenMemoryClient = {
  async read(request) {
    calls.push({ method: "read", payload: request });
    return {
      id: request.id ?? "memory-from-slug",
      slug: request.slug,
      bodyMarkdown: "Stored externally in Den.",
    };
  },
  async search(request) {
    calls.push({ method: "search", payload: request });
    return {
      memories: [{ id: "memory-search", summary: request.query, score: 0.8 }],
      total: 1,
    };
  },
  async recall(request) {
    calls.push({ method: "recall", payload: request });
    return {
      memories: [{ id: "memory-recall", summary: request.prompt, score: 0.9 }],
      total: 1,
    };
  },
  async store(request) {
    calls.push({ method: "store", payload: request });
    return {
      accepted: true,
      memory: {
        id: "memory-store",
        title: request.title,
        bodyMarkdown: request.bodyMarkdown,
      },
    };
  },
  async propose(request) {
    calls.push({ method: "propose", payload: request });
    return {
      accepted: true,
      proposalId: "proposal-store",
      memory: {
        id: request.targetMemoryId ?? "candidate-memory",
        title: request.title,
      },
    };
  },
};
const baseContext = {
  client,
  policy: {
    mode: "permissive",
    defaultAudience: ["project"],
    defaultRole: "prime",
    defaultMemoryMode: "project",
  },
  runtimeContext: {
    projectId: "rusty-crew",
    taskId: 2899,
  },
  session: {
    sessionId: "session-memory" as SessionId,
    agentId: "agent-memory" as AgentId,
    profileId: "prime" as ProfileId,
    kind: "full",
  },
} satisfies DenMemoryToolContext;

const tools = resolveDenMemoryTools(baseContext);
assert.deepEqual(
  tools.map((tool) => tool.name),
  [
    "den_memory_recall",
    "den_memory_read",
    "den_memory_search",
    "den_memory_store",
    "den_memory_propose",
  ],
);

const recall = await denMemoryRecallTool(baseContext).execute("recall", {
  prompt: "What owns memory?",
});
assertDetails(recall.details, {
  operation: "recall",
  action: "read",
});
assert.equal(
  (calls.find((call) => call.method === "recall")?.payload as { role?: string })
    .role,
  "prime",
);

const read = await denMemoryReadTool(baseContext).execute("read", {
  id: "memory-1",
});
assertDetails(read.details, {
  operation: "read",
  action: "read",
});

const search = await denMemorySearchTool(baseContext).execute("search", {
  query: "Den memory",
  audience: ["agent"],
});
assertDetails(search.details, {
  operation: "search",
  action: "read",
});
assert.deepEqual(
  (
    calls.find((call) => call.method === "search")?.payload as {
      audience?: string[];
    }
  ).audience,
  ["agent"],
);

const store = await denMemoryStoreTool(baseContext).execute("store", {
  title: "Store Memory",
  bodyMarkdown: "Store directly for full/prime contexts.",
});
assertDetails(store.details, {
  operation: "store",
  action: "stored",
});
assert.equal(
  (
    calls.find((call) => call.method === "store")
      ?.payload as DenMemoryStoreRequest
  ).context?.sessionId,
  "session-memory",
);

const candidateContext = {
  ...baseContext,
  policy: { mode: "candidate" as const },
};
const candidateStore = await denMemoryStoreTool(candidateContext).execute(
  "candidate-store",
  {
    title: "Candidate Memory",
    bodyMarkdown: "Candidate only.",
  },
);
assertDetails(candidateStore.details, {
  operation: "store",
  action: "proposed",
});

const workerContext = {
  ...baseContext,
  policy: { mode: "permissive" as const },
  session: {
    ...baseContext.session,
    profileId: "review" as ProfileId,
    kind: "worker" as const,
  },
};
const workerStore = await denMemoryStoreTool(workerContext).execute(
  "worker-store",
  {
    bodyMarkdown: "Worker cannot store directly.",
  },
);
assertDetails(workerStore.details, {
  operation: "store",
  action: "proposed",
});

const manualStore = await denMemoryStoreTool({
  ...baseContext,
  policy: { mode: "manual" },
}).execute("manual-store", {
  bodyMarkdown: "Manual review required.",
});
assertDetails(manualStore.details, {
  operation: "store",
  action: "denied",
});
assert.equal(
  manualStore.details.reasonCode,
  "den_memory_manual_review_required",
);

const metadataPropose = await denMemoryProposeTool({
  ...baseContext,
  policy: { mode: "metadata" },
}).execute("metadata-propose", {
  bodyMarkdown: "No writes in metadata mode.",
});
assertDetails(metadataPropose.details, {
  operation: "propose",
  action: "denied",
});

const offRecall = await denMemoryRecallTool({
  ...baseContext,
  policy: { mode: "off" },
}).execute("off-recall", {
  prompt: "No memory.",
});
assert.equal(offRecall.details.reasonCode, "den_memory_policy_off");

const missingClient = await denMemorySearchTool({
  policy: { mode: "permissive" },
}).execute("missing-client", {
  query: "unavailable",
});
assert.equal(missingClient.details.reasonCode, "den_memory_client_unavailable");
assert.equal(missingClient.details.retryable, true);

const proposal = await denMemoryProposeTool(baseContext).execute("propose", {
  proposalKind: "update",
  targetMemoryId: "memory-1",
  title: "Update Memory",
  bodyMarkdown: "Propose update.",
});
assertDetails(proposal.details, {
  operation: "propose",
  action: "proposed",
});
assert.equal(
  (lastCall("propose")?.payload as DenMemoryProposeRequest).targetMemoryId,
  "memory-1",
);

console.log(
  JSON.stringify(
    {
      tools: tools.map((tool) => tool.name),
      calls: calls.map((call) => call.method),
      directStore:
        store.details.action === "stored" && store.details.ok === true,
      candidateStore: candidateStore.details.action,
      workerStore: workerStore.details.action,
      manualStore: manualStore.details.reasonCode,
      metadataPropose: metadataPropose.details.reasonCode,
      offRecall: offRecall.details.reasonCode,
      missingClient: missingClient.details.reasonCode,
    },
    null,
    2,
  ),
);

function assertDetails(
  details: DenMemoryToolDetails | undefined,
  expected: Pick<DenMemoryToolDetails, "operation" | "action">,
): asserts details is DenMemoryToolDetails {
  assert.ok(details);
  assert.equal(details.operation, expected.operation);
  assert.equal(details.action, expected.action);
}

function lastCall(
  method: string,
): { method: string; payload: unknown } | undefined {
  return [...calls].reverse().find((call) => call.method === method);
}
