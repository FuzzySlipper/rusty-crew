import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  buildProfileRoleAssembly,
  FileSessionTodoStore,
  MemorySessionTodoStore,
  renderSessionTodoContext,
  todoTool,
} from "./index.js";
import type { ProfileId } from "@rusty-crew/contracts";
import type { LoadedProfileContext } from "./profile-loading.js";

let nowMs = Date.parse("2026-06-20T08:00:00.000Z");
const store = new MemorySessionTodoStore({
  now: () => new Date(nowMs),
  maxItems: 3,
});
const tool = todoTool({ store, sessionId: "session-alpha" });

const empty = await tool.execute("read-empty", { action: "read" });
assert.equal(empty.details.ok, true);
assert.equal(empty.details.state?.items.length, 0);

const replaced = await tool.execute("replace", {
  action: "replace",
  ttlMs: 1_000,
  items: [
    {
      id: "one",
      title: "Draft plan",
      status: "pending",
      notes: "Session-local only.",
    },
    {
      id: "two",
      title: "Verify implementation",
      status: "pending",
    },
  ],
});
assert.equal(replaced.details.ok, true);
assert.equal(replaced.details.state?.items.length, 2);
assert.equal(replaced.details.state?.expiresAt, "2026-06-20T08:00:01.000Z");

const merged = await tool.execute("merge", {
  action: "merge",
  ttlMs: 1_000,
  items: [
    {
      id: "two",
      title: "Verify implementation",
      status: "in_progress",
      notes: "Smoke running.",
    },
    {
      id: "three",
      title: "Document behavior",
      status: "pending",
    },
  ],
});
assert.equal(merged.details.ok, true);
assert.deepEqual(
  merged.details.state?.items.map((item) => [item.id, item.status]),
  [
    ["one", "pending"],
    ["two", "in_progress"],
    ["three", "pending"],
  ],
);

const overCap = await tool.execute("over-cap", {
  action: "replace",
  items: [
    { id: "one", title: "One", status: "pending" },
    { id: "two", title: "Two", status: "pending" },
    { id: "three", title: "Three", status: "pending" },
    { id: "four", title: "Four", status: "pending" },
  ],
});
assert.equal(overCap.details.ok, false);
assert.equal(overCap.details.reasonCode, "todo_too_many_items");

const todoContext = renderSessionTodoContext(merged.details.state);
assert.match(todoContext ?? "", /Session-local planning notes only/);
assert.match(todoContext ?? "", /\[in_progress\] two/);

const assembled = buildProfileRoleAssembly(sampleProfileContext(), {
  todoContext,
});
assert.match(
  assembled.roleAssembly.instructions ?? "",
  /These are not Den tasks/,
);

nowMs = Date.parse("2026-06-20T08:00:02.000Z");
const expired = await tool.execute("read-expired", { action: "read" });
assert.equal(expired.details.state?.items.length, 0);

const fileRoot = mkdtempSync(join(tmpdir(), "rusty-crew-todos-"));
try {
  nowMs = Date.parse("2026-06-20T09:00:00.000Z");
  const firstFileStore = new FileSessionTodoStore({
    rootDir: fileRoot,
    now: () => new Date(nowMs),
  });
  const fileTool = todoTool({
    store: firstFileStore,
    sessionId: "session-restart-safe",
  });
  const persisted = await fileTool.execute("file-replace", {
    action: "replace",
    ttlMs: 1_000,
    items: [{ id: "persisted", title: "Survive restart", status: "pending" }],
  });
  assert.equal(persisted.details.ok, true);

  const secondFileStore = new FileSessionTodoStore({
    rootDir: fileRoot,
    now: () => new Date(nowMs),
  });
  assert.equal(
    secondFileStore.read("session-restart-safe").items[0]?.id,
    "persisted",
  );

  nowMs = Date.parse("2026-06-20T09:00:02.000Z");
  assert.equal(secondFileStore.read("session-restart-safe").items.length, 0);
} finally {
  rmSync(fileRoot, { recursive: true, force: true });
}

console.log(
  JSON.stringify(
    {
      replaced: replaced.details.state?.items.length,
      merged: merged.details.state?.items.length,
      overCap: overCap.details.reasonCode,
      contextIncluded: /Session Todo/.test(
        assembled.roleAssembly.instructions ?? "",
      ),
      expired: expired.details.state?.items.length,
      fileStoreRestartSafe: true,
    },
    null,
    2,
  ),
);

function sampleProfileContext(): LoadedProfileContext {
  return {
    profile: {
      profileId: "prime-profile" as ProfileId,
      displayName: "Prime",
      modelConfig: { provider: "den-router", modelName: "local" },
    },
    skills: [],
    toolSelection: {
      profileId: "prime-profile" as ProfileId,
      catalogId: "default",
      toolProfile: { tools: [] },
      inventory: {
        selectedTools: [],
        selectedBindings: [],
        selectedDescriptors: [],
        items: [],
      },
    },
  };
}
