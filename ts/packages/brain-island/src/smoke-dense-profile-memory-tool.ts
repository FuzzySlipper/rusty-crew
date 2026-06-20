import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { denseProfileMemoryTool } from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-dense-memory-"));

try {
  const firstBridge = await loadNativeBridge();
  const engine = await firstBridge.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-20T06:00:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  const readOnly = denseProfileMemoryTool({
    client: firstBridge,
    mode: "read_only",
    profileId: "prime-profile",
  });
  const deniedWrite = await readOnly.execute("denied", {
    action: "add",
    key: "style",
    content: "concise",
  });
  assert.equal(deniedWrite.details.ok, false);
  assert.equal(
    deniedWrite.details.reasonCode,
    "dense_profile_memory_read_only",
  );

  const tool = denseProfileMemoryTool({
    client: firstBridge,
    mode: "read_write",
    profileId: "prime-profile",
    caps: {
      maxRecordsPerProfile: 3,
      maxKeyBytes: 32,
      maxContentBytes: 128,
    },
  });
  const added = await tool.execute("add", {
    action: "add",
    key: "style",
    content: "prefers concise handoffs",
    metadata: { source: "smoke" },
  });
  assert.equal(added.details.ok, true);
  assert.equal(record(added.details.result).revision, 1);

  const replaced = await tool.execute("replace", {
    action: "replace",
    key: "style",
    content: "prefers concise handoffs with citations",
    expectedRevision: 1,
    metadata: { source: "smoke-replace" },
  });
  assert.equal(replaced.details.ok, true);
  assert.equal(record(replaced.details.result).revision, 2);

  const stale = await tool.execute("stale", {
    action: "replace",
    key: "style",
    content: "stale write",
    expectedRevision: 1,
  });
  assert.equal(stale.details.ok, false);
  assert.equal(stale.details.reasonCode, "dense_profile_memory_call_failed");

  const userMemory = await tool.execute("add-user", {
    action: "add",
    targetType: "user",
    targetId: "den-user-alpha",
    key: "salutation",
    content: "likes direct updates",
  });
  assert.equal(userMemory.details.ok, true);
  assert.equal(record(userMemory.details.result).targetType, "user");

  await firstBridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });

  const secondBridge = await loadNativeBridge();
  const restartedEngine = await secondBridge.initializeEngine({
    engineDataDir: root,
    clock: { fixed: "2026-06-20T06:05:00Z" },
    defaultTurnBudget: 4,
    defaultIdleTimeoutMs: 1_000,
  });
  const restartedTool = denseProfileMemoryTool({
    client: secondBridge,
    mode: "read_write",
    profileId: "prime-profile",
  });
  const readBack = await restartedTool.execute("read", {
    action: "read",
    key: "style",
  });
  assert.equal(readBack.details.ok, true);
  assert.match(record(readBack.details.result).content, /citations/);

  const listed = await restartedTool.execute("list", {
    action: "list",
    limit: 10,
  });
  assert.equal(listed.details.ok, true);
  assert.equal(Array.isArray(listed.details.result), true);
  assert.equal((listed.details.result as unknown[]).length, 2);

  const removed = await restartedTool.execute("remove", {
    action: "remove",
    key: "style",
    expectedRevision: 2,
  });
  assert.equal(removed.details.ok, true);
  assert.equal(record(removed.details.result).key, "style");

  await secondBridge.shutdownEngine({
    engine: restartedEngine,
    drainTimeoutMs: 1_000,
  });

  console.log(
    JSON.stringify(
      {
        deniedWrite: deniedWrite.details.reasonCode,
        addedRevision: record(added.details.result).revision,
        replacedRevision: record(replaced.details.result).revision,
        stale: stale.details.reasonCode,
        userTarget: record(userMemory.details.result).targetType,
        restartedContent: record(readBack.details.result).content,
        listed: (listed.details.result as unknown[]).length,
        removed: record(removed.details.result).key,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

function record(value: unknown): {
  revision: number;
  targetType: string;
  key: string;
  content: string;
} {
  assert.equal(typeof value, "object");
  assert.notEqual(value, null);
  return value as {
    revision: number;
    targetType: string;
    key: string;
    content: string;
  };
}
