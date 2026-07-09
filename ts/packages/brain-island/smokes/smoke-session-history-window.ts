import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-session-history-"));
const native = await loadNativeBridge();

try {
  const engine = await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-22T00:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  try {
    await native.createSession({
      sessionId: "history-session" as SessionId,
      agentId: "history-agent" as AgentId,
      profileId: "history-profile" as ProfileId,
      kind: "full",
      historyWindow: {
        maxMessages: 1,
      },
    });
    await native.routeAgentMessage(
      "operator" as AgentId,
      "history-agent" as AgentId,
      "first message",
      "first",
    );
    await native.routeAgentMessage(
      "operator" as AgentId,
      "history-agent" as AgentId,
      "second message",
      "second",
    );
    assert.deepEqual(await projectedMessageBodies(), ["second message"]);
  } finally {
    await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }

  const restarted = await native.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-22T00:00:01Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  try {
    const session = (await native.listSessions()).find(
      (candidate) => candidate.sessionId === "history-session",
    );
    assert.equal(session?.historyWindow?.maxMessages, 1);
    assert.deepEqual(await projectedMessageBodies(), ["second message"]);
    console.log(
      JSON.stringify(
        {
          sessionId: session?.sessionId,
          maxMessages: session?.historyWindow?.maxMessages,
          projectedMessages: await projectedMessageBodies(),
        },
        null,
        2,
      ),
    );
  } finally {
    await native.shutdownEngine({ engine: restarted, drainTimeoutMs: 1_000 });
  }
} finally {
  rmSync(root, { force: true, recursive: true });
}

async function projectedMessageBodies(): Promise<string[]> {
  const raw = JSON.parse(
    new TextDecoder().decode(
      await native.projectBodyStateJson("history-session" as SessionId),
    ),
  ) as {
    pending_messages: { body: string }[];
  };
  return raw.pending_messages.map((message) => message.body);
}
