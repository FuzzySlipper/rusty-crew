import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import {
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

test("profile config migration is structured and idempotent", () => {
  const root = mkdtempSync(join(tmpdir(), "rusty-crew-chat-brain-migration-"));
  const profilesDir = join(root, "config", "profiles");
  mkdirSync(profilesDir, { recursive: true });
  const legacyPath = join(profilesDir, "legacy.json");
  const responsesPath = join(profilesDir, "responses.json");
  writeFileSync(
    legacyPath,
    `${JSON.stringify({ profileId: "legacy", brain: { module: "pi-agent", strategy: "default" } }, null, 2)}\n`,
  );
  writeFileSync(
    responsesPath,
    `${JSON.stringify({ profileId: "responses", brain: { module: "openai-responses", strategy: "replay" } }, null, 2)}\n`,
  );

  try {
    const first = runMigration(root);
    assert.deepEqual(first.results[0].migrated, ["legacy.json"]);
    assert.equal(
      JSON.parse(readFileSync(legacyPath, "utf8")).brain.module,
      "chat-completions",
    );
    assert.equal(
      JSON.parse(readFileSync(responsesPath, "utf8")).brain.module,
      "openai-responses",
    );

    const second = runMigration(root);
    assert.deepEqual(second.results[0].migrated, []);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function runMigration(root) {
  return JSON.parse(
    execFileSync(
      process.execPath,
      ["tools/migrate-chat-completions-brain-config.mjs", root],
      { cwd: process.cwd(), encoding: "utf8" },
    ),
  );
}
