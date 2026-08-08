import assert from "node:assert/strict";
import test from "node:test";

import { migrateLegacyFullSessionWorkspaces } from "../src/migrate-session-workspaces.js";

test("legacy full-session migration writes only explicit session workspaces", () => {
  const source = {
    schemaVersion: 1,
    profiles: [{ profileId: "shared-profile", runtime: { model: "test" } }],
    sessions: [
      {
        sessionId: "legacy-full",
        profileId: "shared-profile",
        kind: "full",
        resourceLimits: { workdir: "/legacy", maxDurationMs: 5 },
      },
      {
        sessionId: "current-full",
        profileId: "shared-profile",
        kind: "full",
        workspaceCwd: "/already-explicit",
      },
      {
        sessionId: "delegated",
        profileId: "shared-profile",
        kind: "delegated",
        resourceLimits: { workdir: "/delegated-scope" },
      },
    ],
  };

  const migrated = migrateLegacyFullSessionWorkspaces(
    source,
    "/explicit/migration",
  );
  assert.deepEqual(migrated.migratedSessionIds, ["legacy-full"]);
  const sessions = migrated.config.sessions as Array<Record<string, unknown>>;
  assert.equal(sessions[0]?.workspaceCwd, "/explicit/migration");
  assert.deepEqual(sessions[0]?.resourceLimits, { maxDurationMs: 5 });
  assert.deepEqual(sessions[2]?.resourceLimits, {
    workdir: "/delegated-scope",
  });
  assert.deepEqual(source.sessions[0]?.resourceLimits, {
    workdir: "/legacy",
    maxDurationMs: 5,
  });
  assert.equal("workspaceCwd" in source.profiles[0]!, false);
});

test("migration refuses a missing or relative workspace", () => {
  const config = { sessions: [] };
  assert.throws(
    () => migrateLegacyFullSessionWorkspaces(config, "relative/repo"),
    /explicit absolute path/,
  );
  assert.throws(
    () => migrateLegacyFullSessionWorkspaces(config, ""),
    /explicit absolute path/,
  );
});
