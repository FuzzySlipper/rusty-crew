#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const engineDir = "crates/core/core-engine/src";
const libPath = join(engineDir, "lib.rs");
const testDir = join(engineDir, "tests");
const requiredDomains = [
  "agent_route_activation.rs",
  "body.rs",
  "bootstrap.rs",
  "brain_runtime.rs",
  "chat.rs",
  "delegation.rs",
  "github_gate.rs",
  "maintenance.rs",
  "memory.rs",
  "profile_admin.rs",
  "provider_runtime.rs",
  "roleplay.rs",
  "runtime_admin.rs",
  "scheduler.rs",
  "sessions.rs",
];
const forbiddenCatchAllFiles = ["helpers.rs", "utils.rs", "common.rs"];
const forbiddenDependencyFragments = [
  "core-bridge",
  "service-host",
  "adapter-",
  "@rusty-crew",
];
const failures = [];

const lib = readFileSync(libPath, "utf8");
checkCeiling(libPath, lib, { lines: 650, bytes: 32 * 1024 });
if (/\#\[cfg\(test\)\]\s*mod\s+tests\s*\{/.test(lib)) {
  failures.push(`${libPath} must not contain an inline tests module`);
}
for (const block of lib.matchAll(/impl\s+CoreEngine\s*\{([\s\S]*?)\n\}/g)) {
  const lines = lineCount(block[0]);
  if (lines > 250) {
    failures.push(
      `${libPath} contains an impl CoreEngine block with ${lines} lines`,
    );
  }
}

for (const file of requiredDomains) {
  const path = join(engineDir, file);
  if (!existsSync(path))
    failures.push(`missing CoreEngine domain module ${path}`);
}
for (const file of forbiddenCatchAllFiles) {
  const path = join(engineDir, file);
  if (existsSync(path))
    failures.push(`forbidden catch-all CoreEngine module ${path}`);
}

for (const file of readdirSync(engineDir).filter((name) =>
  name.endsWith(".rs"),
)) {
  const path = join(engineDir, file);
  const source = readFileSync(path, "utf8");
  if (file !== "lib.rs") checkCeiling(path, source, { lines: 1_500 });
  for (const fragment of forbiddenDependencyFragments) {
    if (source.includes(fragment)) {
      failures.push(
        `${path} contains forbidden dependency fragment ${fragment}`,
      );
    }
  }
}

if (!existsSync(testDir) || !statSync(testDir).isDirectory()) {
  failures.push(`missing domain test directory ${testDir}`);
} else {
  for (const file of readdirSync(testDir).filter((name) =>
    name.endsWith(".rs"),
  )) {
    const path = join(testDir, file);
    checkCeiling(path, readFileSync(path, "utf8"), { lines: 1_800 });
  }
}

if (failures.length > 0) {
  console.error("CoreEngine decomposition ratchet failed:\n");
  for (const failure of failures) console.error(`- ${failure}`);
  console.error(
    "\nMove behavior into an existing cohesive domain module or document and add a deliberate follow-up split before changing a ceiling.",
  );
  process.exit(1);
}

console.log(
  JSON.stringify({
    libLines: lineCount(lib),
    libBytes: Buffer.byteLength(lib),
    domainModules: requiredDomains.length,
    testModules: readdirSync(testDir).filter((name) => name.endsWith(".rs"))
      .length,
    inlineTests: false,
    catchAllModules: false,
  }),
);

function checkCeiling(path, source, ceiling) {
  const lines = lineCount(source);
  const bytes = Buffer.byteLength(source);
  if (ceiling.lines !== undefined && lines > ceiling.lines) {
    failures.push(`${path} has ${lines} lines; ceiling is ${ceiling.lines}`);
  }
  if (ceiling.bytes !== undefined && bytes > ceiling.bytes) {
    failures.push(`${path} has ${bytes} bytes; ceiling is ${ceiling.bytes}`);
  }
}

function lineCount(source) {
  return source.length === 0 ? 0 : source.split("\n").length;
}
