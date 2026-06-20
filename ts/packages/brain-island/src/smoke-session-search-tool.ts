import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { sessionSearchTool } from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-session-search-"));

try {
  const native = await loadNativeBridge();
  const engine = await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-20T07:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  await native.createSession({
    sessionId: "session-alpha",
    agentId: "agent-alpha",
    profileId: "prime-profile",
    kind: "full",
  });
  await native.createSession({
    sessionId: "session-beta",
    agentId: "agent-beta",
    profileId: "review-profile",
    kind: "worker",
  });
  await native.routeAgentMessage(
    "agent-alpha",
    "agent-beta",
    "Investigate the phoenix runtime handoff.",
    "corr-phoenix",
  );
  await native.routeAgentMessage(
    "agent-beta",
    "agent-alpha",
    "Phoenix handoff review completed.",
    "corr-phoenix",
  );

  const tool = sessionSearchTool({ client: native, maxBodyChars: 24 });
  const messages = await tool.execute("search-messages", {
    query: "Investigate",
    rowType: "message",
    agentId: "agent-beta",
    limit: 5,
  });
  assert.equal(messages.details.ok, true);
  assert.equal(messages.details.results?.length, 1);
  assert.equal(messages.details.results?.[0]?.rowType, "message");
  assert.equal(messages.details.results?.[0]?.truncated, true);

  const sessions = await tool.execute("search-sessions", {
    query: "prime-profile",
    rowType: "session",
    limit: 5,
  });
  assert.equal(sessions.details.ok, true);
  assert.equal(sessions.details.results?.[0]?.rowType, "session");
  assert.equal(sessions.details.results?.[0]?.sessionId, "session-alpha");

  const missingClient = await sessionSearchTool({}).execute("missing", {
    query: "phoenix",
  });
  assert.equal(missingClient.details.ok, false);
  assert.equal(
    missingClient.details.reasonCode,
    "runtime_search_client_unavailable",
  );

  console.log(
    JSON.stringify(
      {
        messages: messages.details.results?.length,
        messageSnippet: messages.details.results?.[0]?.bodySnippet,
        sessions: sessions.details.results?.length,
        missingClient: missingClient.details.reasonCode,
      },
      null,
      2,
    ),
  );

  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
} finally {
  rmSync(root, { recursive: true, force: true });
}
