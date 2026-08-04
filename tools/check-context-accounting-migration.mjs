import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (relativePath) =>
  readFileSync(resolve(root, relativePath), "utf8");

const inventory = read("docs/context-accounting-migration-inventory.md");
const fallbackId = "fallback_chars_words_v1";
const requiredPaths = [
  "ts/packages/brain-island/src/context-estimate.ts",
  "ts/packages/brain-island/src/service-rusty-view-chat-operations.ts",
  "ts/packages/brain-island/src/package-surface/profile-context.ts",
  "ts/packages/brain-island/smokes/smoke-context-estimate.ts",
  "ts/smokes/brain-island/smoke-rusty-view-chat-context.ts",
  "fixtures/external-cassettes/rusty-view-chat-api/roleplay-turn-readback.redacted.json",
  "crates/core/core-persistence/src/sqlite_integration_tests.rs",
  "ts/packages/brain-island/src/tool-profile-prompt-authority.ts",
];

for (const path of requiredPaths) {
  assert.ok(inventory.includes(`\`${path}\``), `inventory missing ${path}`);
  read(path);
}

const estimator = read("ts/packages/brain-island/src/context-estimate.ts");
assert.ok(estimator.includes(fallbackId));
assert.ok(
  estimator.includes(
    "approximate_chars_div4_and_words_4over3_from_chat_events",
  ),
);
assert.ok(
  read(
    "ts/packages/brain-island/src/service-rusty-view-chat-operations.ts",
  ).includes("compatibility-only"),
);
assert.ok(
  read(
    "ts/packages/brain-island/src/tool-profile-prompt-authority.ts",
  ).includes("diagnostic_estimator"),
);
assert.ok(
  read("crates/core/core-persistence/src/sqlite_integration_tests.rs").includes(
    fallbackId,
  ),
  "historical SQLite fixture inventory must remain explicit",
);

const serviceApp = read("ts/packages/brain-island/src/service-app.ts");
assert.ok(
  !serviceApp.includes("./context-estimate.js"),
  "service-app must not become a fallback estimator consumer",
);

const brainSources = [
  "crates/brains/brain-runtime/src/context_accounting.rs",
  "crates/brains/chat-completions/src/lib.rs",
  "crates/brains/openai-responses/src/lib.rs",
].map(read);
assert.ok(
  brainSources.every((source) => !source.includes(fallbackId)),
  "Rust brain authority must not depend on the legacy TypeScript estimator",
);

for (const marker of [
  "Rust contract",
  "Current Call-Site Inventory",
  "Removal Gates",
  "Compatibility-only",
  "native snapshot",
  "Offline and Postgres",
]) {
  assert.ok(inventory.includes(marker), `inventory missing marker ${marker}`);
}

console.log("context accounting migration inventory passed");
