import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createWebSearchProvider,
  resolveToolSession,
  resolveWebTools,
  webSearchTool,
} from "./index.js";
import type {
  BrainWakeInput,
  WebSearchProvider,
  WebSearchToolDetails,
} from "./index.js";

const providerCalls: Array<{ query: string; maxResults: number }> = [];
const fakeProvider: WebSearchProvider = {
  name: "fake",
  async search(query, maxResults) {
    providerCalls.push({ query, maxResults });
    return [
      {
        title: " Rusty Crew ",
        url: "https://example.com/rusty-crew",
        snippet: "  Project runtime  ",
      },
      {
        title: "Dropped because max results",
        url: "https://example.com/drop",
        snippet: "extra",
      },
    ];
  },
};

const directResult = await webSearchTool({
  provider: fakeProvider,
  searchDefaultLimit: 7,
  searchMaxResults: 8,
}).execute("search-direct", {
  query: "  rusty crew runtime  ",
  max_results: 99,
});

assert.equal(directResult.details.ok, true);
assert.equal(directResult.details.provider, "fake");
assert.equal(directResult.details.query, "rusty crew runtime");
assert.equal(directResult.details.maxResults, 8);
assert.equal(directResult.details.results.length, 2);
assert.equal(directResult.details.results[0]?.title, "Rusty Crew");
assert.equal(directResult.details.results[0]?.snippet, "Project runtime");
assert.equal(providerCalls[0]?.maxResults, 8);

const failedSearch = await webSearchTool({
  provider: {
    name: "failing",
    async search() {
      throw new Error("provider offline");
    },
  },
}).execute("search-failure", {
  query: "provider failure",
});
assert.equal(failedSearch.details.ok, false);
assert.equal(failedSearch.details.provider, "failing");
assert.equal(failedSearch.details.error, "provider offline");
assert.deepEqual(failedSearch.details.results, []);

const searxngRequests: string[] = [];
const searxngProvider = createWebSearchProvider({
  env: { RUSTY_CREW_SEARXNG_URL: "https://search.example/search" },
  fetchImpl: async (input) => {
    searxngRequests.push(String(input));
    return Response.json({
      results: [
        {
          title: "SearXNG Result",
          url: "https://example.com/searxng",
          content: "SearXNG snippet",
        },
      ],
    });
  },
});

const searxngResults = await searxngProvider.search("policy", 3);
assert.equal(searxngProvider.name, "searxng");
assert.equal(searxngResults[0]?.url, "https://example.com/searxng");
assert.match(searxngRequests[0] ?? "", /format=json/);
assert.match(searxngRequests[0] ?? "", /q=policy/);

const duckDuckGoProvider = createWebSearchProvider({
  env: {},
  fetchImpl: async () =>
    new Response(`
      <a rel="nofollow" class="result__a" href="/l/?uddg=https%3A%2F%2Fexample.com%2Fddg">DDG &amp; Result</a>
      <a class="result__snippet">Useful &lt;snippet&gt;</a>
    `),
});

const duckDuckGoResults = await duckDuckGoProvider.search("fallback", 1);
assert.equal(duckDuckGoProvider.name, "duckduckgo_html");
assert.equal(duckDuckGoResults[0]?.title, "DDG & Result");
assert.equal(duckDuckGoResults[0]?.url, "https://example.com/ddg");
assert.equal(duckDuckGoResults[0]?.snippet, "Useful <snippet>");

const toolSession = resolveToolSession({
  wake: wakeWithWebSearch(),
  resolveTools: () => resolveWebTools({ provider: fakeProvider }),
});

assert.deepEqual(
  toolSession.tools.map((tool) => tool.name),
  ["web_search"],
);
assert.equal(toolSession.items[0]?.status, "callable");

const selectedResult = await toolSession.tools[0]?.execute("search-selected", {
  query: "selected brain",
  max_results: 1,
});
const selectedDetails = selectedResult?.details as
  | WebSearchToolDetails
  | undefined;
assert.equal(
  selectedDetails?.results[0]?.url,
  "https://example.com/rusty-crew",
);

console.log(
  JSON.stringify(
    {
      directProvider: directResult.details.provider,
      searxngProvider: searxngProvider.name,
      duckDuckGoProvider: duckDuckGoProvider.name,
      selectedTools: toolSession.tools.map((tool) => tool.name),
    },
    null,
    2,
  ),
);

function wakeWithWebSearch(): BrainWakeInput {
  return {
    wakeId: "wake-web-search",
    sessionId: "session-web-search" as SessionId,
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId: "session-web-search" as SessionId,
        agentId: "agent-web-search" as AgentId,
        profileId: "profile-web-search" as ProfileId,
        kind: "full",
        status: "active",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00.000Z",
        lastActiveAt: "2026-06-20T00:00:00.000Z",
        resourceLimits: {},
        toolProfile: {
          tools: [
            {
              name: "web_search",
              description:
                "Search the public web through the configured provider.",
            },
          ],
        },
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 60_000,
        maxQueuedMessages: 8,
      },
    },
    systemPrompt: "",
    roleAssembly: {
      instructions: "Use web_search when needed.",
    },
  };
}
