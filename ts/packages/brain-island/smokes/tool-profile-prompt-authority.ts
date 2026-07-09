import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  requiredToolProfilePromptSurfacePaths,
  toolProfilePromptSurfaceClassifications,
  type ToolProfilePromptSurfaceClassification,
} from "../src/tool-profile-prompt-authority.js";

const smokeDir = dirname(fileURLToPath(import.meta.url));
const srcDir = join(smokeDir, "../src");
const repoRoot = join(smokeDir, "../../../..");
const docsPath = join(
  repoRoot,
  "docs/typescript-tool-profile-prompt-surface-inventory-2026-07-08.md",
);
const docs = readFileSync(docsPath, "utf8");

const allowedClassifications = new Set<ToolProfilePromptSurfaceClassification>([
  "execution_wrapper",
  "prompt_renderer",
  "adapter_glue",
  "provider_client_implementation",
  "diagnostic_estimator",
  "temporary_policy_facade",
]);

const recordsByPath = new Map(
  toolProfilePromptSurfaceClassifications.map((record) => [
    record.path,
    record,
  ]),
);

assert.equal(
  recordsByPath.size,
  toolProfilePromptSurfaceClassifications.length,
  "surface classification paths must be unique",
);

for (const path of requiredToolProfilePromptSurfacePaths) {
  const record = recordsByPath.get(path);
  assert.ok(record, `${path} must have a surface classification record`);
  assert.ok(
    existsSync(join(srcDir, path)),
    `${path} must still exist beside the authority inventory`,
  );
  assert.ok(
    allowedClassifications.has(record.classification),
    `${path} has unknown classification ${record.classification}`,
  );
  assert.ok(
    record.allowedTypeScriptAuthority.length >= 40,
    `${path} must document allowed TypeScript authority`,
  );
  assert.ok(
    record.requiredRustBoundary.length >= 40,
    `${path} must document the required Rust boundary`,
  );
  assert.ok(docs.includes(`\`${path}\``), `${path} must be listed in docs`);
  assert.ok(
    docs.includes(record.classification),
    `${path} classification ${record.classification} must appear in docs`,
  );
}

for (const record of toolProfilePromptSurfaceClassifications) {
  assert.ok(
    requiredToolProfilePromptSurfacePaths.includes(
      record.path as (typeof requiredToolProfilePromptSurfacePaths)[number],
    ),
    `${record.path} is classified but not in the required path list`,
  );
  if (record.classification !== "temporary_policy_facade") continue;
  assert.ok(
    record.remainingPolicy,
    `${record.path} temporary policy facade must document remaining policy disposition`,
  );
}

const remainingPolicyRecords = toolProfilePromptSurfaceClassifications.filter(
  (record) => record.remainingPolicy !== undefined,
);
assert.ok(
  remainingPolicyRecords.length >= 1,
  "at least one remaining policy owner should be explicitly documented",
);
for (const record of remainingPolicyRecords) {
  assert.ok(
    docs.includes(`\`${record.path}\``) && docs.includes("Remaining Policy"),
    `${record.path} remaining policy must be documented`,
  );
  const policy = record.remainingPolicy;
  assert.ok(policy, `${record.path} remaining policy should be present`);
  assert.ok(
    policy.disposition === "intentional" || policy.followUpTaskId !== undefined,
    `${record.path} remaining policy needs an intentional reason or follow-up task`,
  );
  assert.ok(
    policy.note.length >= 40,
    `${record.path} remaining policy note must explain the reason`,
  );
}

console.log(
  JSON.stringify(
    {
      classifiedSurfaces: toolProfilePromptSurfaceClassifications.length,
      classifications: [...allowedClassifications].sort(),
      remainingPolicySurfaces: remainingPolicyRecords.map(
        (record) => record.path,
      ),
    },
    null,
    2,
  ),
);
