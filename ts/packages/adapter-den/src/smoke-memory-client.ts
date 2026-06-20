import assert from "node:assert/strict";
import {
  createDenMemoryClient,
  DenMemoryClientError,
  type DenMemoryFetch,
} from "./index.js";

const calls: Array<{
  url: string;
  method: string;
  authorization?: string;
  body: Record<string, unknown>;
}> = [];
const fetchImpl: DenMemoryFetch = async (input, init) => {
  const url = new URL(String(input));
  const body =
    typeof init?.body === "string"
      ? (JSON.parse(init.body) as Record<string, unknown>)
      : {};
  calls.push({
    url: url.toString(),
    method: init?.method ?? "GET",
    authorization: header(init?.headers, "authorization"),
    body,
  });

  switch (url.pathname) {
    case "/memory/read":
      return json({
        ok: true,
        data: {
          id: body["id"],
          title: "Read Memory",
          bodyMarkdown: "Remember the boundary.",
          sourceRefs: body["sourceRefs"],
        },
      });
    case "/memory/search":
      return json({
        memories: [
          {
            id: "memory-search-1",
            summary: `search:${body["query"]}`,
            score: 0.9,
          },
        ],
        total: 1,
      });
    case "/memory/recall":
      return json({
        ok: true,
        data: {
          memories: [
            {
              id: "memory-recall-1",
              summary: `recall:${body["prompt"]}`,
              audience: body["audience"],
            },
          ],
          total: 1,
        },
      });
    case "/memory/store":
      assert.deepEqual(body["context"], {
        projectId: "rusty-crew",
        taskId: 2898,
        sessionId: "session-memory",
        profileId: "prime",
      });
      return json({
        ok: true,
        data: {
          accepted: true,
          memory: {
            id: "memory-store-1",
            title: body["title"],
            bodyMarkdown: body["bodyMarkdown"],
            sourceRefs: body["sourceRefs"],
          },
        },
      });
    case "/memory/propose":
      return json({
        accepted: true,
        proposalId: "proposal-1",
        message: "proposal accepted",
      });
    case "/memory/error":
      return json(
        {
          ok: false,
          error: {
            code: "failed_precondition",
            reason_code: "den_memory_unavailable",
            message: "Den Memories unavailable",
            retryable: true,
          },
        },
        503,
      );
    default:
      return json({ error: { message: "missing" } }, 404);
  }
};

const client = createDenMemoryClient({
  baseUrl: "http://den.local",
  bearerToken: "memory-token",
  fetchImpl,
  paths: {
    read: "/memory/read",
    search: "/memory/search",
    recall: "/memory/recall",
    store: "/memory/store",
    propose: "/memory/propose",
  },
});

const read = await client.read({ id: "memory-1" });
assert.equal(read.id, "memory-1");

const search = await client.search({
  query: "authority",
  audience: ["project"],
  role: "prime",
  mode: "project",
  context: {
    projectId: "rusty-crew",
    taskId: 2898,
  },
});
assert.equal(search.memories[0]?.summary, "search:authority");

const recall = await client.recall({
  prompt: "What owns Den memory?",
  audience: ["project", "agent"],
  limit: 2,
});
assert.deepEqual(recall.memories[0]?.audience, ["project", "agent"]);

const store = await client.store({
  title: "Memory Boundary",
  bodyMarkdown: "Den Memories are Den-owned external memory.",
  audience: ["project"],
  mode: "project",
  context: {
    projectId: "rusty-crew",
    taskId: 2898,
    sessionId: "session-memory",
    profileId: "prime",
  },
  sourceRefs: [
    {
      kind: "den_task",
      ref: "2898",
      label: "Den Memories client task",
    },
  ],
  metadata: {
    adapter: "rusty-crew",
  },
});
assert.equal(store.accepted, true);
assert.equal(store.memory?.sourceRefs?.[0]?.ref, "2898");

const proposal = await client.propose({
  proposalKind: "store",
  title: "Candidate Memory",
  bodyMarkdown: "Propose this memory.",
});
assert.equal(proposal.proposalId, "proposal-1");

const failingClient = createDenMemoryClient({
  baseUrl: "http://den.local",
  fetchImpl,
  paths: {
    read: "/memory/error",
  },
});
let caught: unknown;
try {
  await failingClient.read({ id: "memory-error" });
} catch (error) {
  caught = error;
}
assert.ok(caught instanceof DenMemoryClientError);
assert.equal(caught.code, "failed_precondition");
assert.equal(caught.options.reasonCode, "den_memory_unavailable");
assert.equal(caught.options.retryable, true);

assert.equal(calls[0]?.method, "POST");
assert.equal(calls[0]?.authorization, "Bearer memory-token");
assert.equal(
  calls.some((call) => call.url.endsWith("/memory/store")),
  true,
);

console.log(
  JSON.stringify(
    {
      read: read.id,
      searchTotal: search.total,
      recallTotal: recall.total,
      storeAccepted: store.accepted,
      proposal: proposal.proposalId,
      calls: calls.length,
      errorCode: caught instanceof DenMemoryClientError ? caught.code : "none",
    },
    null,
    2,
  ),
);

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
    },
  });
}

function header(
  headers: HeadersInit | undefined,
  name: string,
): string | undefined {
  if (!headers) {
    return undefined;
  }
  if (headers instanceof Headers) {
    return headers.get(name) ?? undefined;
  }
  if (Array.isArray(headers)) {
    return headers.find(([key]) => key.toLowerCase() === name)?.[1];
  }
  return headers[name];
}
