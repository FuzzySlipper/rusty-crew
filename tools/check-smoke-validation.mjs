#!/usr/bin/env node

import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { buildCatalog } from "./smoke-runner.mjs";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const allowedVerifyLanes = new Set(["offline", "native-offline"]);
const rootSmokeAliasCeiling = 130;
const forbiddenVerifyRequirements = new Set([
  "den",
  "local-router",
  "service-startup",
  "postgres",
  "rusty-view",
  "live-provider",
  "openai-oauth",
  "telegram-config",
]);

function readPackageJson() {
  return JSON.parse(readFileSync(join(repoRoot, "package.json"), "utf8"));
}

export function extractSmokeScriptNames(scriptCommand) {
  const names = [];
  for (const match of scriptCommand.matchAll(
    /npm\s+run\s+(smoke:[^\s&|;]+)/g,
  )) {
    names.push(match[1].replace(/^smoke:/, ""));
  }
  return names;
}

export function summarizeCatalog(catalog) {
  const summary = {
    total: catalog.length,
    byScope: {},
    byLane: {},
    byCategory: {},
    byRequirement: {},
  };

  for (const entry of catalog) {
    increment(summary.byScope, entry.scope);
    increment(summary.byLane, entry.lane);
    increment(summary.byCategory, entry.category);
    for (const requirement of entry.requirements) {
      increment(summary.byRequirement, requirement);
    }
  }

  return summary;
}

function increment(bucket, key) {
  bucket[key] = (bucket[key] ?? 0) + 1;
}

export function auditSmokeValidation({
  packageJson = readPackageJson(),
  catalog = buildCatalog(),
} = {}) {
  const verifyTs = packageJson.scripts?.["verify:ts"] ?? "";
  const verifySmokeNames = extractSmokeScriptNames(verifyTs);
  const violations = [];
  const checked = [];
  const rootAliases = catalog.filter((entry) => entry.scope === "root-alias");

  if (rootAliases.length > rootSmokeAliasCeiling) {
    violations.push({
      name: "root-smoke-alias-ceiling",
      reason: `root package exposes ${rootAliases.length} smoke aliases; add package-local scripts discoverable by npm run smoke -- --list instead of exceeding the ceiling ${rootSmokeAliasCeiling}`,
    });
  }

  for (const name of verifySmokeNames) {
    const matches = catalog.filter(
      (entry) => entry.scope === "root-alias" && entry.name === name,
    );
    if (matches.length !== 1) {
      violations.push({
        name,
        reason: `verify:ts references smoke:${name}, but the smoke catalog found ${matches.length} root aliases`,
      });
      continue;
    }

    const entry = matches[0];
    checked.push(entry);
    if (!allowedVerifyLanes.has(entry.lane)) {
      violations.push({
        name,
        reason: `verify:ts smoke:${name} is lane ${entry.lane}; allowed lanes are ${[...allowedVerifyLanes].join(", ")}`,
      });
    }
    const forbidden = entry.requirements.filter((requirement) =>
      forbiddenVerifyRequirements.has(requirement),
    );
    if (forbidden.length > 0) {
      violations.push({
        name,
        reason: `verify:ts smoke:${name} has forbidden offline-gate requirements: ${forbidden.join(", ")}`,
      });
    }
  }

  return {
    checked,
    summary: summarizeCatalog(catalog),
    verifySmokeNames,
    violations,
  };
}

function printAudit(audit) {
  console.log(
    `[smoke-validation] catalog total=${audit.summary.total} scopes=${formatBucket(audit.summary.byScope)} lanes=${formatBucket(audit.summary.byLane)}`,
  );
  console.log(
    `[smoke-validation] verify:ts smokes=${audit.checked.map((entry) => `${entry.name}:${entry.lane}`).join(", ")}`,
  );
  if (audit.violations.length === 0) {
    console.log(
      "[smoke-validation] offline gate contains only deterministic offline/native-offline smoke checks",
    );
    return;
  }

  console.error("[smoke-validation] offline gate violations:");
  for (const violation of audit.violations) {
    console.error(`- ${violation.reason}`);
  }
}

function formatBucket(bucket) {
  return Object.entries(bucket)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, count]) => `${key}:${count}`)
    .join(",");
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const audit = auditSmokeValidation();
  printAudit(audit);
  if (audit.violations.length > 0) {
    process.exit(1);
  }
}
