#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { cwd, exit } from "node:process";

const root = cwd();
const scopePath = join(root, "governance", "storage-scope.toml");
const scope = parseStorageScopeToml(readFileSync(scopePath, "utf8"));
const ownedTables = new Set(scope.scopes.flatMap((entry) => entry.owns));
const ignoredCreatedTables = new Set(scope.rules.testOnlyTables);
const generatedPrefixes = scope.scopes.flatMap(
  (entry) => entry.generatedPrefixes,
);
const violations = [];

checkRustSqlPlacement();
checkTypescriptHasNoRawSql();
checkCreatedTablesAreOwned();

if (violations.length > 0) {
  console.error("Storage scope boundary check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  exit(1);
}

console.log(
  JSON.stringify(
    {
      checkedScopes: scope.scopes.length,
      ownedTables: ownedTables.size,
      generatedPrefixes,
      rustSqlRoot: "crates/core/core-persistence",
      typescriptRawSql: "forbidden",
    },
    null,
    2,
  ),
);

function checkRustSqlPlacement() {
  for (const file of findFiles(join(root, "crates"), [".rs"])) {
    const relativePath = normalizePath(relative(root, file));
    if (relativePath.startsWith("crates/core/core-persistence/")) continue;
    const source = readFileSync(file, "utf8");
    if (containsRawSql(source)) {
      violations.push(
        `${relativePath} contains raw SQL; SQL belongs behind crates/core/core-persistence`,
      );
    }
  }
}

function checkTypescriptHasNoRawSql() {
  const packagesDir = join(root, "ts", "packages");
  if (!existsSync(packagesDir)) return;
  for (const file of findFiles(packagesDir, [".ts"])) {
    const relativePath = normalizePath(relative(root, file));
    const source = readFileSync(file, "utf8");
    if (containsRawSql(source)) {
      violations.push(
        `${relativePath} contains raw SQL; TypeScript must use storage APIs instead`,
      );
    }
  }
}

function checkCreatedTablesAreOwned() {
  const persistenceDir = join(root, "crates", "core", "core-persistence");
  for (const file of findFiles(persistenceDir, [".rs"])) {
    const relativePath = normalizePath(relative(root, file));
    const source = readFileSync(file, "utf8");
    for (const table of createdTables(source)) {
      if (table.includes("{")) continue;
      if (ignoredCreatedTables.has(table)) continue;
      if (ownedTables.has(table)) continue;
      if (generatedPrefixes.some((prefix) => table.startsWith(prefix))) {
        continue;
      }
      violations.push(
        `${relativePath} creates table ${table}, but governance/storage-scope.toml does not assign it to a scope`,
      );
    }
  }
}

function containsRawSql(source) {
  return /\b(?:CREATE\s+TABLE|CREATE\s+(?:UNIQUE\s+)?INDEX|CREATE\s+TRIGGER|ALTER\s+TABLE|DROP\s+TABLE|INSERT\s+INTO|UPDATE\s+[A-Za-z_][A-Za-z0-9_]*\s+SET|DELETE\s+FROM|SELECT\b[\s\S]{0,240}\bFROM\s+[A-Za-z_][A-Za-z0-9_]*)\b/i.test(
    source,
  );
}

function createdTables(source) {
  const tables = [];
  const pattern =
    /\bCREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:(?:\{schema\}|["`]?\{schema\}["`]?)\.)?["`]?([A-Za-z_{][A-Za-z0-9_{}]*)["`]?\b/g;
  for (const match of source.matchAll(pattern)) {
    if (match[1]) tables.push(match[1]);
  }
  return [...new Set(tables)].sort();
}

function findFiles(dir, extensions) {
  const files = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      files.push(...findFiles(path, extensions));
    } else if (extensions.some((extension) => entry.endsWith(extension))) {
      files.push(path);
    }
  }
  return files.sort();
}

function parseStorageScopeToml(text) {
  const scopes = [];
  const rules = {};
  let current;

  for (let index = 0; index < text.length; ) {
    const lineEnd = text.indexOf("\n", index);
    const rawLine =
      lineEnd === -1 ? text.slice(index) : text.slice(index, lineEnd);
    const line = stripComment(rawLine).trim();
    index = lineEnd === -1 ? text.length : lineEnd + 1;

    if (line === "" || line.startsWith("[")) {
      if (line === "[[scope]]") {
        current = {};
        scopes.push(current);
      } else if (line === "[rules]") {
        current = rules;
      } else if (line.startsWith("[")) {
        current = undefined;
      }
      continue;
    }

    if (!current) continue;
    const assignment = line.match(/^([A-Za-z0-9_]+)\s*=\s*(.+)$/);
    if (!assignment) continue;
    const [, key, rawValue] = assignment;
    if (rawValue.trim() === "[") {
      const parsed = readMultilineArray(text, index);
      current[key] = parsed.value;
      index = parsed.nextIndex;
    } else if (rawValue.trim().startsWith("[")) {
      current[key] = parseTomlArray(rawValue);
    } else {
      current[key] = parseTomlString(rawValue);
    }
  }

  return {
    scopes: scopes.map((entry) => ({
      id: requiredString(entry, "id"),
      owns: requiredStringArray(entry, "owns"),
      generatedPrefixes: stringArray(entry.generated_prefixes),
    })),
    rules: {
      testOnlyTables: stringArray(rules.test_only_tables),
    },
  };
}

function readMultilineArray(text, startIndex) {
  let index = startIndex;
  let body = "";
  while (index < text.length) {
    const lineEnd = text.indexOf("\n", index);
    const line =
      lineEnd === -1 ? text.slice(index) : text.slice(index, lineEnd);
    index = lineEnd === -1 ? text.length : lineEnd + 1;
    body += line;
    if (stripComment(line).includes("]")) break;
  }
  return {
    value: parseTomlArray(`[${body}`),
    nextIndex: index,
  };
}

function parseTomlArray(raw) {
  const body = raw.trim().replace(/^\[/, "").replace(/\]$/, "");
  if (body.trim() === "") return [];
  return body
    .split(",")
    .map((part) => parseTomlString(part.trim()))
    .filter((value) => value !== "");
}

function parseTomlString(raw) {
  const match = raw.trim().match(/^"((?:[^"\\]|\\.)*)"/);
  if (!match) return raw.trim();
  return match[1].replace(/\\"/g, '"').replace(/\\\\/g, "\\");
}

function requiredString(entry, key) {
  const value = entry[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`storage scope entry missing required string ${key}`);
  }
  return value;
}

function requiredStringArray(entry, key) {
  const value = entry[key];
  if (!Array.isArray(value) || value.some((item) => typeof item !== "string")) {
    throw new Error(
      `storage scope entry ${entry.id ?? "<unknown>"} missing required string array ${key}`,
    );
  }
  return value;
}

function stringArray(value) {
  return Array.isArray(value)
    ? value.filter((item) => typeof item === "string")
    : [];
}

function stripComment(line) {
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (char === '"' && line[index - 1] !== "\\") quoted = !quoted;
    if (!quoted && char === "#") return line.slice(0, index);
  }
  return line;
}

function normalizePath(path) {
  return path.split("\\").join("/");
}
