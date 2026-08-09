import assert from "node:assert/strict";
import test from "node:test";

import {
  REVIEW_DEN_REQUIRED_TOOLS,
  serviceReviewDenAuthority,
  validateServiceReviewDenAuthority,
} from "../src/service-review-den-authority.js";

const authority = {
  authorityId: "service-review-den",
  endpointRef: "config://mcp/den",
  serverName: "den" as const,
  toolProfileKey: "direct" as const,
  auditIdentity: "rusty-crew-review-service",
};

test("service review Den authority is unchanged by archive, replacement, rebuild, and empty runtime graphs", () => {
  const beforeArchive = serviceReviewDenAuthority(authority);
  const lifecycleGraphs = [
    { sessions: [], mcpBindings: [] },
    {
      sessions: [{ sessionId: "replacement" }],
      mcpBindings: [{ bindingId: "interactive-replacement" }],
    },
    {
      sessions: [{ sessionId: "rebuilt" }],
      mcpBindings: [],
    },
  ];
  assert.equal(beforeArchive?.bindingId, "service-review-den");
  for (const runtimeGraph of lifecycleGraphs) {
    assert.ok(runtimeGraph.sessions);
    assert.deepEqual(serviceReviewDenAuthority(authority), beforeArchive);
  }
});

test("service review Den authority validates the exact required tool surface", async () => {
  const result = await validateServiceReviewDenAuthority({
    authority,
    now: () => "2026-08-09T08:00:00.000Z",
    listTools: async () => REVIEW_DEN_REQUIRED_TOOLS.map((name) => ({ name })),
  });

  assert.equal(result.status, "ready");
  assert.deepEqual(result.missingTools, []);
  assert.equal(result.auditIdentity, "rusty-crew-review-service");
});

test("service review Den authority reports missing tools and recovers on retry", async () => {
  let restored = false;
  const validate = () =>
    validateServiceReviewDenAuthority({
      authority,
      now: () => "2026-08-09T08:00:00.000Z",
      listTools: async () =>
        (restored
          ? REVIEW_DEN_REQUIRED_TOOLS
          : REVIEW_DEN_REQUIRED_TOOLS.filter(
              (name) => name !== "finalize_review",
            )
        ).map((name) => ({ name })),
    });

  assert.equal((await validate()).status, "missing_tools");
  restored = true;
  assert.equal((await validate()).status, "ready");
});

test("service review Den authority remains durable while its endpoint is unavailable", async () => {
  const result = await validateServiceReviewDenAuthority({
    authority,
    now: () => "2026-08-09T08:00:00.000Z",
    listTools: async () => {
      throw new Error("Den endpoint unavailable");
    },
  });

  assert.equal(result.status, "unavailable");
  assert.equal(result.authorityId, "service-review-den");
  assert.match(result.message, /endpoint unavailable/);
});
