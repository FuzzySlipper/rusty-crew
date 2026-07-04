#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { cwd, exit } from "node:process";

const root = cwd();
const scanRoots = ["crates", "ts/packages"].map((path) => join(root, path));
const extensions = new Set([".rs", ".ts", ".tsx", ".js", ".mjs"]);
const ignoredPathFragments = [
  "/src/smoke-",
  "/smokes/",
  "/test/",
  "/tests/",
  "/target/",
  "/node_modules/",
  "/native/",
];

const forbidden = [
  {
    id: "rust-responses-echo-tool-output",
    pattern: /completed by Rust Responses bridge/g,
  },
  {
    id: "ts-responses-deterministic-tool-output",
    pattern: /completed in deterministic field scaffold/g,
  },
  {
    id: "responses-module-scaffold-success",
    pattern: /responses module scaffold wake completed/g,
  },
  {
    id: "responses-replay-scaffold-success",
    pattern: /responses replay service wake completed/g,
  },
  {
    id: "deterministic-scaffold-comment",
    pattern: /deterministic [^\n]{0,80}scaffold/g,
  },
];

const allowed = [
  {
    id: "rust-responses-echo-tool-output",
    path: "crates/bridge/core-bridge-node/src/lib.rs",
    count: 1,
    reason:
      "legacy blocking bridge fixture hook; service brain uses buffered submit-output path",
  },
  {
    id: "responses-module-scaffold-success",
    path: "crates/bridge/core-bridge-node/src/lib.rs",
    count: 2,
    reason:
      "fake Responses client used by explicit fake-mode smokes through the native bridge",
  },
];

const allowedByKey = new Map(
  allowed.map((entry) => [`${entry.id}:${entry.path}`, entry]),
);
const seenAllowed = new Map();
const violations = [];

for (const rootDir of scanRoots) {
  if (!existsSync(rootDir)) continue;
  for (const file of findFiles(rootDir)) {
    const relativePath = normalizePath(relative(root, file));
    if (ignoredPathFragments.some((fragment) => relativePath.includes(fragment))) {
      continue;
    }
    const source = readFileSync(file, "utf8");
    for (const rule of forbidden) {
      const matches = [...source.matchAll(rule.pattern)];
      for (const match of matches) {
        const allowedKey = `${rule.id}:${relativePath}`;
        const allowedEntry = allowedByKey.get(allowedKey);
        if (allowedEntry) {
          seenAllowed.set(allowedKey, (seenAllowed.get(allowedKey) ?? 0) + 1);
          continue;
        }
        violations.push(
          `${relativePath}:${lineForOffset(source, match.index ?? 0)} matches ${rule.id}: ${JSON.stringify(match[0])}`,
        );
      }
    }
  }
}

for (const entry of allowed) {
  const key = `${entry.id}:${entry.path}`;
  const count = seenAllowed.get(key) ?? 0;
  if (count !== entry.count) {
    violations.push(
      `allowlist drift for ${entry.id} in ${entry.path}: expected ${entry.count}, found ${count}; ${entry.reason}`,
    );
  }
}

if (violations.length > 0) {
  console.error("Production fake/scaffold guard failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  exit(1);
}

console.log(
  JSON.stringify(
    {
      checkedRoots: scanRoots.map((path) => normalizePath(relative(root, path))),
      forbiddenRules: forbidden.map((rule) => rule.id),
      allowedLegacySeams: allowed.map(({ id, path, count, reason }) => ({
        id,
        path,
        count,
        reason,
      })),
    },
    null,
    2,
  ),
);

function findFiles(dir) {
  const files = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      files.push(...findFiles(path));
    } else if (extensions.has(extensionFor(entry))) {
      files.push(path);
    }
  }
  return files.sort();
}

function extensionFor(path) {
  const index = path.lastIndexOf(".");
  return index === -1 ? "" : path.slice(index);
}

function lineForOffset(source, offset) {
  return source.slice(0, offset).split("\n").length;
}

function normalizePath(path) {
  return path.split("\\").join("/");
}
