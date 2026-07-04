import assert from "node:assert/strict";
import type { NativeSimpleKvRecord } from "@rusty-crew/native-bridge";
import { getSceneStateTool, updateSceneStateTool } from "./scene-state-tool.js";

async function runSmoke(): Promise<void> {
  const bridge = new FakeSimpleKvBridge();
  const context = {
    client: bridge,
    session: { sessionId: "rp-session-1" },
    now: () => "2026-07-03T21:00:00Z",
  };

  const empty = await getSceneStateTool(context).execute("get-empty", {});
  assert.equal(empty.details.ok, true);
  assert.deepEqual(empty.details.state, {
    sessionId: "rp-session-1",
    charactersPresent: [],
    activeThreads: [],
  });

  const updated = await updateSceneStateTool(context).execute("update", {
    location: "Moonlit Garden",
    charactersPresent: ["elara", "katheryn"],
    activeThreads: ["missing locket"],
    notes: "The air is warm after rain.",
  });
  assert.equal(updated.details.ok, true);
  assert.equal(updated.details.state?.location, "Moonlit Garden");
  assert.deepEqual(updated.details.state?.charactersPresent, [
    "elara",
    "katheryn",
  ]);
  assert.equal(bridge.records.get(key("rp-session-1"))?.revision, 1);

  const patched = await updateSceneStateTool(context).execute("patch", {
    notes: null,
    activeThreads: ["missing locket", "garden promise"],
  });
  assert.equal(patched.details.ok, true);
  assert.equal(patched.details.state?.notes, undefined);
  assert.deepEqual(patched.details.state?.activeThreads, [
    "missing locket",
    "garden promise",
  ]);
  assert.equal(bridge.records.get(key("rp-session-1"))?.revision, 2);

  const reread = await getSceneStateTool(context).execute("get", {});
  assert.equal(reread.details.ok, true);
  assert.equal(reread.details.state?.location, "Moonlit Garden");

  const denied = await getSceneStateTool({ client: bridge }).execute(
    "missing-session",
    {},
  );
  assert.equal(denied.details.ok, false);
  assert.equal(denied.details.reasonCode, "session_id_missing");

  console.log(
    JSON.stringify(
      {
        location: reread.details.state?.location,
        activeThreads: reread.details.state?.activeThreads,
        revision: reread.details.revision,
        denied: denied.details.reasonCode,
      },
      null,
      2,
    ),
  );
}

class FakeSimpleKvBridge {
  readonly records = new Map<string, NativeSimpleKvRecord>();

  async listSimpleKv(query: {
    scopeType: string;
    scopeId: string;
    keyPrefix?: string;
  }): Promise<NativeSimpleKvRecord[]> {
    return [...this.records.values()].filter(
      (record) =>
        record.scopeType === query.scopeType &&
        record.scopeId === query.scopeId &&
        (!query.keyPrefix || record.key.startsWith(query.keyPrefix)),
    );
  }

  async putSimpleKv(write: {
    scopeType: string;
    scopeId: string;
    key: string;
    valueJson: string;
    now: string;
    expiresAt?: string;
  }): Promise<NativeSimpleKvRecord> {
    const recordKey = key(write.scopeId);
    const existing = this.records.get(recordKey);
    const record: NativeSimpleKvRecord = {
      scopeType: write.scopeType,
      scopeId: write.scopeId,
      key: write.key,
      valueJson: write.valueJson,
      revision: (existing?.revision ?? 0) + 1,
      createdAt: existing?.createdAt ?? write.now,
      updatedAt: write.now,
      expiresAt: write.expiresAt,
    };
    this.records.set(recordKey, record);
    return record;
  }
}

function key(sessionId: string): string {
  return `roleplay_scene_state:${sessionId}:current`;
}

await runSmoke();
