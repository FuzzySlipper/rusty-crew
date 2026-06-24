#!/usr/bin/env node
import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { cwd, exit } from "node:process";

const root = cwd();
const ownershipPath = join(root, "governance", "ownership.toml");
const ownership = parseOwnershipToml(readFileSync(ownershipPath, "utf8"));
const manifests = findCargoManifests(join(root, "crates")).map((path) => {
  const manifest = parseCargoManifest(readFileSync(path, "utf8"));
  return {
    path,
    relativePath: normalizePath(relative(root, path)),
    packageName: manifest.packageName,
    dependencies: manifest.dependencies,
  };
});
const manifestsByPackage = new Map(
  manifests.map((manifest) => [manifest.packageName, manifest]),
);
const internalPackageNames = new Set(
  manifests.map((manifest) => manifest.packageName),
);
const violations = [];

for (const rule of ownership.crates) {
  const manifest = manifestsByPackage.get(rule.name);
  if (!manifest) {
    violations.push(`ownership rule references missing crate ${rule.name}`);
    continue;
  }
  violations.push(...dependencyViolations(manifest, rule.mayNotDependOn));
}

for (const lane of ownership.lanes) {
  const laneManifests = manifests.filter((manifest) =>
    lane.pathPrefixes.some((prefix) =>
      manifest.relativePath.startsWith(prefix),
    ),
  );
  for (const manifest of laneManifests) {
    violations.push(...dependencyViolations(manifest, lane.mayNotDependOn));

    const unexpectedInternal = manifest.dependencies.filter(
      (dependency) =>
        internalPackageNames.has(dependency) &&
        !lane.allowedInternalDependencies.includes(dependency),
    );
    for (const dependency of unexpectedInternal) {
      violations.push(
        `${manifest.packageName} (${manifest.relativePath}) depends on internal crate ${dependency}, which is not approved for lane ${lane.name}`,
      );
    }

    violations.push(
      ...forbiddenSourceReferences(manifest, lane.mayNotDependOn),
    );
  }
}

if (violations.length > 0) {
  console.error("Rust crate boundary check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  exit(1);
}

console.log(
  JSON.stringify(
    {
      checkedCrates: manifests.length,
      explicitRules: ownership.crates.length,
      lanes: ownership.lanes.map((lane) => ({
        name: lane.name,
        matchedCrates: manifests.filter((manifest) =>
          lane.pathPrefixes.some((prefix) =>
            manifest.relativePath.startsWith(prefix),
          ),
        ).length,
      })),
    },
    null,
    2,
  ),
);

function dependencyViolations(manifest, forbidden) {
  const dependencies = new Set(manifest.dependencies);
  return forbidden
    .filter((dependency) => dependencies.has(dependency))
    .map(
      (dependency) =>
        `${manifest.packageName} (${manifest.relativePath}) must not depend on ${dependency}`,
    );
}

function findCargoManifests(dir) {
  const entries = readdirSync(dir);
  const manifests = [];
  for (const entry of entries) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      manifests.push(...findCargoManifests(path));
    } else if (entry === "Cargo.toml") {
      manifests.push(path);
    }
  }
  return manifests.sort();
}

function forbiddenSourceReferences(manifest, forbidden) {
  const sourceDir = join(dirname(manifest.path), "src");
  if (!existsDirectory(sourceDir)) return [];

  const source = readRustSource(sourceDir);
  const references = forbidden
    .map((crateName) => crateName.replaceAll("-", "_"))
    .filter((moduleName) =>
      source.some(({ text }) =>
        new RegExp(`(^|[^A-Za-z0-9_])${escapeRegExp(moduleName)}\\s*::`).test(
          text,
        ),
      ),
    );

  return references.map(
    (moduleName) =>
      `${manifest.packageName} (${manifest.relativePath}) references forbidden Rust module ${moduleName}::`,
  );
}

function readRustSource(dir) {
  const entries = readdirSync(dir);
  const sources = [];
  for (const entry of entries) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      sources.push(...readRustSource(path));
    } else if (entry.endsWith(".rs")) {
      sources.push({ path, text: readFileSync(path, "utf8") });
    }
  }
  return sources;
}

function existsDirectory(path) {
  try {
    return statSync(path).isDirectory();
  } catch {
    return false;
  }
}

function parseOwnershipToml(text) {
  const crates = [];
  const lanes = [];
  let current;

  for (let index = 0; index < text.length; ) {
    const lineEnd = text.indexOf("\n", index);
    const rawLine =
      lineEnd === -1 ? text.slice(index) : text.slice(index, lineEnd);
    const line = stripComment(rawLine).trim();
    index = lineEnd === -1 ? text.length : lineEnd + 1;

    if (line === "" || line.startsWith("[")) {
      if (line === "[[crate]]") {
        current = {};
        crates.push(current);
      } else if (line === "[[lane]]") {
        current = {};
        lanes.push(current);
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
    crates: crates.map((entry) => ({
      name: requiredString(entry, "name"),
      mayNotDependOn: requiredStringArray(entry, "may_not_depend_on"),
    })),
    lanes: lanes.map((entry) => ({
      name: requiredString(entry, "name"),
      pathPrefixes: requiredStringArray(entry, "path_prefixes").map(
        normalizePath,
      ),
      allowedInternalDependencies: stringArray(
        entry.allowed_internal_dependencies,
      ),
      mayNotDependOn: requiredStringArray(entry, "may_not_depend_on"),
    })),
  };
}

function parseCargoManifest(text) {
  let section = "";
  let packageName;
  const dependencies = [];
  for (const rawLine of text.split(/\r?\n/)) {
    const line = stripComment(rawLine).trim();
    const sectionMatch = line.match(/^\[([^\]]+)\]$/);
    if (sectionMatch) {
      section = sectionMatch[1];
      continue;
    }
    const assignment = line.match(/^([A-Za-z0-9_-]+)\s*=/);
    if (!assignment) continue;
    const key = assignment[1];
    if (section === "package" && key === "name") {
      packageName = parseTomlString(line.split("=").slice(1).join("="));
    }
    if (
      section === "dependencies" ||
      section === "dev-dependencies" ||
      section === "build-dependencies"
    ) {
      dependencies.push(key);
    }
  }
  if (!packageName) throw new Error("Cargo manifest is missing package.name");
  return { packageName, dependencies };
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
  const match = raw.match(/^\[\s*([\s\S]*?)\s*\]/);
  if (!match) throw new Error(`unsupported array value: ${raw}`);
  const values = [];
  const regex = /"([^"\\]*(?:\\.[^"\\]*)*)"/g;
  let value;
  while ((value = regex.exec(match[1])) !== null) {
    values.push(value[1].replace(/\\"/g, '"'));
  }
  return values;
}

function parseTomlString(raw) {
  const match = raw.trim().match(/^"([^"\\]*(?:\\.[^"\\]*)*)"/);
  if (!match) throw new Error(`unsupported string value: ${raw}`);
  return match[1].replace(/\\"/g, '"');
}

function stripComment(line) {
  let inString = false;
  for (let i = 0; i < line.length; i += 1) {
    if (line[i] === '"' && line[i - 1] !== "\\") inString = !inString;
    if (line[i] === "#" && !inString) return line.slice(0, i);
  }
  return line;
}

function requiredString(entry, key) {
  if (typeof entry[key] !== "string") {
    throw new Error(`ownership entry missing string ${key}`);
  }
  return entry[key];
}

function requiredStringArray(entry, key) {
  const value = stringArray(entry[key]);
  if (value.length === 0) {
    throw new Error(`ownership entry missing array ${key}`);
  }
  return value;
}

function stringArray(value) {
  return Array.isArray(value) ? value : [];
}

function normalizePath(path) {
  return path.replaceAll("\\", "/");
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
