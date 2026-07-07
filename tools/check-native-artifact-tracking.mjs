#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { exit } from "node:process";

const trackedNativePaths = git(["ls-files", "ts/packages/native-bridge/native"])
  .split("\n")
  .filter(Boolean)
  .sort();

const expectedTracked = ["ts/packages/native-bridge/native/index.d.ts"];
const violations = [];

if (JSON.stringify(trackedNativePaths) !== JSON.stringify(expectedTracked)) {
  violations.push(
    [
      "native bridge generated artifact tracking drift",
      `expected tracked native paths: ${JSON.stringify(expectedTracked)}`,
      `actual tracked native paths: ${JSON.stringify(trackedNativePaths)}`,
      "only the generated declaration surface is committed; .node and .js outputs are build artifacts",
    ].join("; "),
  );
}

for (const artifactPath of [
  "ts/packages/native-bridge/native/index.js",
  "ts/packages/native-bridge/native/index.linux-x64-gnu.node",
]) {
  if (!gitCheckIgnore(artifactPath)) {
    violations.push(`${artifactPath} must be ignored build output`);
  }
}

if (violations.length > 0) {
  console.error("Native bridge artifact tracking check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  exit(1);
}

console.log(
  JSON.stringify(
    {
      trackedNativePaths,
      ignoredBuildArtifacts: [
        "ts/packages/native-bridge/native/index.js",
        "ts/packages/native-bridge/native/index.linux-x64-gnu.node",
      ],
    },
    null,
    2,
  ),
);

function git(args) {
  return execFileSync("git", args, { encoding: "utf8" }).trim();
}

function gitCheckIgnore(path) {
  try {
    execFileSync("git", ["check-ignore", "--quiet", path], {
      stdio: "ignore",
    });
    return true;
  } catch {
    return false;
  }
}
