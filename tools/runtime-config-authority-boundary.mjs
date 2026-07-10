import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const path = "ts/packages/brain-island/src/service-runtime-config.ts";
const source = await readFile(path, "utf8");
const lines = source.split("\n").length;

assert.ok(
  lines <= 2_075,
  `${path} grew to ${lines} lines; update the authority boundary before raising the ceiling`,
);
assert.match(source, /planRuntimeGraphWithRust\(/);
assert.doesNotMatch(source, /planRuntimeConfigWithRust\(/);

for (const legacyAuthority of [
  "emptyRuntimeConfig",
  "validateRuntimeConfig",
  "runtimeStorageConfig",
  "runtimeStorageBackend",
  "runtimePostgresBootMode",
  "validateRuntimeStorageConfig",
  "configuredScheduledJob",
  "configuredBrain",
  "configuredSession",
  "configuredChannelBinding",
  "configuredMcpBinding",
  "runtimeConfigFromNativeDraft",
  "backgroundReviewScheduledJob",
]) {
  assert.ok(
    !source.includes(`${legacyAuthority}(`),
    `${legacyAuthority} restored TypeScript runtime graph authority`,
  );
}

console.log(
  `runtime config authority boundary passed (${lines} lines, Rust graph planner active)`,
);
