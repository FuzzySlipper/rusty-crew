#!/usr/bin/env node

import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));

function readJson(path) {
  return JSON.parse(readFileSync(path, "utf8"));
}

function packageSlug(packageName) {
  return packageName.replace(/^@rusty-crew\//, "");
}

function scriptNameFromKey(scriptKey) {
  return scriptKey === "smoke" ? "default" : scriptKey.replace(/^smoke:/, "");
}

export function classifySmoke(entry) {
  const text =
    `${entry.name} ${entry.package} ${entry.script} ${entry.command}`.toLowerCase();
  const requirements = new Set();
  const tags = new Set();
  let category = "package-integration";

  if (text.includes("postgres")) {
    category = "storage";
    requirements.add("postgres");
    tags.add("storage");
  }
  if (
    text.includes("service") ||
    text.includes("admin") ||
    text.includes("runtime-health") ||
    text.includes("direct-debug")
  ) {
    category = "service-host";
    requirements.add("service-startup");
    tags.add("service");
  }
  if (
    text.includes("den") ||
    text.includes("channel") ||
    text.includes("observation") ||
    text.includes("assignment") ||
    text.includes("successor")
  ) {
    category = "den-adapter";
    requirements.add("den");
    tags.add("den");
  }
  if (text.includes("router")) {
    requirements.add("local-router");
    tags.add("router");
  }
  if (text.includes("bridge") || text.includes("native")) {
    category = "native-bridge";
    requirements.add("native-build");
    tags.add("bridge");
  }
  if (text.includes("rusty-view") || text.includes("operator-surfaces")) {
    category = "rusty-view";
    requirements.add("rusty-view");
    tags.add("ui");
  }
  if (text.includes("live")) {
    requirements.add("live-provider");
    tags.add("live");
  }
  if (text.includes("telegram")) {
    category = "adapter-integration";
    requirements.add("telegram-config");
    tags.add("telegram");
  }
  if (text.includes("mcp")) {
    category = "adapter-integration";
    tags.add("mcp");
  }
  if (text.includes("tool")) {
    tags.add("tools");
  }
  if (text.includes("profile")) {
    tags.add("profiles");
  }
  if (
    text.includes("memory") ||
    text.includes("curator") ||
    text.includes("lore")
  ) {
    tags.add("memory");
  }
  if (text.includes("delegat") || text.includes("worker")) {
    tags.add("delegation");
  }
  if (text.includes("responses")) {
    tags.add("responses");
    if (text.includes("live")) {
      requirements.add("openai-oauth");
    }
  }
  if (requirements.size === 0) {
    requirements.add("none");
  }

  return {
    category,
    lane: smokeExecutionLane({ category, requirements, text }),
    requirements: [...requirements].sort(),
    tags: [...tags].sort(),
  };
}

export function smokeExecutionLane({ category, requirements, text = "" }) {
  if (requirements.has("rusty-view")) {
    return "rusty-view-certification";
  }
  if (
    requirements.has("live-provider") ||
    requirements.has("openai-oauth") ||
    requirements.has("telegram-config")
  ) {
    return "live-provider";
  }
  if (requirements.has("service-startup")) {
    return text.includes("debug") ? "debug-service" : "local-service";
  }
  if (
    requirements.has("den") ||
    requirements.has("local-router") ||
    requirements.has("postgres")
  ) {
    return "local-infrastructure";
  }
  if (category === "native-bridge" || requirements.has("native-build")) {
    return "native-offline";
  }
  return "offline";
}

export function buildCatalog() {
  const entries = [];
  const rootPackage = readJson(join(repoRoot, "package.json"));

  for (const [script, command] of Object.entries(rootPackage.scripts ?? {})) {
    if (!script.startsWith("smoke:")) {
      continue;
    }
    const name = script.replace(/^smoke:/, "");
    const entry = {
      name,
      scope: "root-alias",
      package: "root",
      script,
      command: `npm run ${script}`,
    };
    entries.push({ ...entry, ...classifySmoke(entry) });
  }

  const packagesDir = join(repoRoot, "ts", "packages");
  for (const packageDir of readdirSync(packagesDir).sort()) {
    const packageJsonPath = join(packagesDir, packageDir, "package.json");
    if (!existsSync(packageJsonPath)) {
      continue;
    }
    const packageJson = readJson(packageJsonPath);
    const packageName = packageJson.name ?? packageDir;
    const slug = packageSlug(packageName);
    for (const [script, command] of Object.entries(packageJson.scripts ?? {})) {
      if (!script.startsWith("smoke")) {
        continue;
      }
      const shortName = scriptNameFromKey(script);
      const entry = {
        name: `${slug}:${shortName}`,
        scope: "package",
        package: packageName,
        script,
        command: `npm run ${script} -w ${packageName}`,
        source: sourcePathFromCommand(packageDir, command),
      };
      entries.push({ ...entry, ...classifySmoke(entry) });
    }
  }

  return entries.sort((left, right) => left.name.localeCompare(right.name));
}

function sourcePathFromCommand(packageDir, command) {
  const match = command.match(/(?:^|\s)tsx\s+([^\s]+)/);
  if (!match) {
    return undefined;
  }
  return relative(
    repoRoot,
    join(repoRoot, "ts", "packages", packageDir, match[1]),
  );
}

export function parseArgs(argv) {
  const parsed = {
    list: false,
    json: false,
    filters: {},
    name: undefined,
    passThrough: [],
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--") {
      parsed.passThrough = argv.slice(index + 1);
      break;
    }
    if (arg === "--list") {
      parsed.list = true;
      continue;
    }
    if (arg === "--json") {
      parsed.json = true;
      continue;
    }
    if (arg === "--help" || arg === "-h") {
      parsed.help = true;
      continue;
    }
    const flagMatch = arg.match(
      /^--(package|category|lane|tag|requirement)=(.+)$/,
    );
    if (flagMatch) {
      parsed.filters[flagMatch[1]] = flagMatch[2];
      continue;
    }
    if (
      ["--package", "--category", "--lane", "--tag", "--requirement"].includes(
        arg,
      )
    ) {
      const key = arg.replace(/^--/, "");
      parsed.filters[key] = argv[index + 1];
      index += 1;
      continue;
    }
    if (!parsed.name) {
      parsed.name = arg;
      continue;
    }
    parsed.passThrough.push(arg);
  }

  return parsed;
}

export function matchesFilters(entry, filters) {
  if (
    filters.package &&
    entry.package !== filters.package &&
    packageSlug(entry.package) !== filters.package
  ) {
    return false;
  }
  if (filters.category && entry.category !== filters.category) {
    return false;
  }
  if (filters.lane && entry.lane !== filters.lane) {
    return false;
  }
  if (filters.tag && !entry.tags.includes(filters.tag)) {
    return false;
  }
  if (
    filters.requirement &&
    !entry.requirements.includes(filters.requirement)
  ) {
    return false;
  }
  return true;
}

function printHelp() {
  console.log(`Usage:
  npm run smoke -- --list [--package <pkg>] [--category <category>] [--lane <lane>] [--tag <tag>]
  npm run smoke -- <name> [-- additional args]

Examples:
  npm run smoke -- --list --package brain-island
  npm run smoke -- --list --lane offline
  npm run smoke -- --list --lane rusty-view-certification
  npm run smoke -- brain
  npm run smoke -- adapter-den:default
`);
}

function printList(entries, json) {
  if (json) {
    console.log(JSON.stringify(entries, null, 2));
    return;
  }

  const rows = entries.map((entry) => ({
    name: entry.name,
    package: packageSlug(entry.package),
    category: entry.category,
    lane: entry.lane,
    requirements: entry.requirements.join(","),
    tags: entry.tags.join(","),
  }));
  const widths = {
    name: Math.max("name".length, ...rows.map((row) => row.name.length)),
    package: Math.max(
      "package".length,
      ...rows.map((row) => row.package.length),
    ),
    category: Math.max(
      "category".length,
      ...rows.map((row) => row.category.length),
    ),
    lane: Math.max("lane".length, ...rows.map((row) => row.lane.length)),
    requirements: Math.max(
      "requirements".length,
      ...rows.map((row) => row.requirements.length),
    ),
  };

  console.log(
    `${"name".padEnd(widths.name)}  ${"package".padEnd(widths.package)}  ${"category".padEnd(widths.category)}  ${"lane".padEnd(widths.lane)}  ${"requirements".padEnd(widths.requirements)}  tags`,
  );
  console.log(
    `${"-".repeat(widths.name)}  ${"-".repeat(widths.package)}  ${"-".repeat(widths.category)}  ${"-".repeat(widths.lane)}  ${"-".repeat(widths.requirements)}  ----`,
  );
  for (const row of rows) {
    console.log(
      `${row.name.padEnd(widths.name)}  ${row.package.padEnd(widths.package)}  ${row.category.padEnd(widths.category)}  ${row.lane.padEnd(widths.lane)}  ${row.requirements.padEnd(widths.requirements)}  ${row.tags}`,
    );
  }
}

export function resolveEntry(entries, name) {
  const exact = entries.filter((entry) => entry.name === name);
  if (exact.length === 1) {
    return exact[0];
  }
  const rootAlias = entries.filter(
    (entry) => entry.scope === "root-alias" && entry.name === name,
  );
  if (rootAlias.length === 1) {
    return rootAlias[0];
  }
  const short = entries.filter((entry) => entry.name.endsWith(`:${name}`));
  if (short.length === 1) {
    return short[0];
  }
  if (short.length > 1) {
    throw new Error(
      `Smoke name "${name}" is ambiguous: ${short.map((entry) => entry.name).join(", ")}`,
    );
  }
  return undefined;
}

async function runCommand(entry, passThrough) {
  const command =
    passThrough.length > 0
      ? `${entry.command} -- ${passThrough.map(shellQuote).join(" ")}`
      : entry.command;
  console.log(`[smoke] ${entry.name}`);
  console.log(
    `[smoke] category=${entry.category} lane=${entry.lane} requirements=${entry.requirements.join(",")} tags=${entry.tags.join(",") || "none"}`,
  );
  console.log(`[smoke] ${command}`);
  const child = spawn(command, {
    cwd: repoRoot,
    env: process.env,
    shell: true,
    stdio: "inherit",
  });
  const code = await new Promise((resolve) => child.on("close", resolve));
  process.exitCode = code ?? 1;
}

function shellQuote(value) {
  if (/^[A-Za-z0-9_/:=.-]+$/.test(value)) {
    return value;
  }
  return `'${value.replaceAll("'", "'\\''")}'`;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const args = parseArgs(process.argv.slice(2));
  const catalog = buildCatalog();
  const filtered = catalog.filter((entry) =>
    matchesFilters(entry, args.filters),
  );

  if (args.help || (!args.list && !args.name)) {
    printHelp();
    process.exit(args.help ? 0 : 1);
  }

  if (args.list) {
    printList(filtered, args.json);
    process.exit(0);
  }

  const entry = resolveEntry(filtered, args.name);
  if (!entry) {
    console.error(
      `Unknown smoke "${args.name}". Use "npm run smoke -- --list" to inspect available smokes.`,
    );
    process.exit(1);
  }

  await runCommand(entry, args.passThrough);
}
