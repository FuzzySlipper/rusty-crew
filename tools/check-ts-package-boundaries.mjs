#!/usr/bin/env node
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { cwd, exit } from "node:process";

const root = cwd();
const packagesDir = join(root, "ts", "packages");
const packages = readWorkspacePackages(packagesDir);
const packagesByName = new Map(packages.map((pkg) => [pkg.name, pkg]));
const violations = [];

const adapterPackages = [
  "@rusty-crew/adapter-den",
  "@rusty-crew/adapter-mcp",
  "@rusty-crew/adapter-telegram",
  "@rusty-crew/adapter-tui",
];

expectNoDependencies("@rusty-crew/brain-island", [
  "@rusty-crew/service-host",
  ...adapterPackages,
]);
for (const adapterName of adapterPackages) {
  expectNoDependencies(adapterName, [
    "@rusty-crew/brain-island",
    "@rusty-crew/service-host",
    ...adapterPackages.filter((candidate) => candidate !== adapterName),
  ]);
}

expectDependencies("@rusty-crew/service-host", [
  "@rusty-crew/brain-island",
  "@rusty-crew/native-bridge",
  "@rusty-crew/contracts",
  "@rusty-crew/adapter-den",
  "@rusty-crew/adapter-mcp",
  "@rusty-crew/adapter-telegram",
]);

expectNoSourceImports("@rusty-crew/brain-island", [
  "@rusty-crew/service-host",
  ...adapterPackages,
]);
for (const adapterName of adapterPackages) {
  expectNoSourceImports(adapterName, [
    "@rusty-crew/brain-island",
    "@rusty-crew/service-host",
    ...adapterPackages.filter((candidate) => candidate !== adapterName),
  ]);
}

if (violations.length > 0) {
  console.error("TypeScript package boundary check failed:");
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  exit(1);
}

console.log(
  JSON.stringify(
    {
      checkedPackages: packages.length,
      brainIslandForbiddenDependencies: [
        "@rusty-crew/service-host",
        ...adapterPackages,
      ],
      serviceHostCompositionDependencies: dependenciesFor(
        "@rusty-crew/service-host",
      ),
    },
    null,
    2,
  ),
);

function readWorkspacePackages(dir) {
  return readdirSync(dir)
    .sort()
    .flatMap((entry) => {
      const packageJsonPath = join(dir, entry, "package.json");
      if (!existsSync(packageJsonPath)) return [];
      const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
      return [
        {
          name: packageJson.name,
          dir: join(dir, entry),
          manifestPath: packageJsonPath,
          packageJson,
        },
      ];
    });
}

function expectNoDependencies(packageName, forbidden) {
  const dependencies = new Set(dependenciesFor(packageName));
  for (const dependency of forbidden) {
    if (dependencies.has(dependency)) {
      violations.push(`${packageName} must not depend on ${dependency}`);
    }
  }
}

function expectDependencies(packageName, expected) {
  const dependencies = new Set(dependenciesFor(packageName));
  for (const dependency of expected) {
    if (!dependencies.has(dependency)) {
      violations.push(`${packageName} should compose ${dependency}`);
    }
  }
}

function dependenciesFor(packageName) {
  const pkg = packagesByName.get(packageName);
  if (!pkg) {
    violations.push(`package boundary rule references missing ${packageName}`);
    return [];
  }
  return Object.keys({
    ...(pkg.packageJson.dependencies ?? {}),
    ...(pkg.packageJson.peerDependencies ?? {}),
    ...(pkg.packageJson.devDependencies ?? {}),
  }).sort();
}

function expectNoSourceImports(packageName, forbidden) {
  const pkg = packagesByName.get(packageName);
  if (!pkg) return;
  const sourceDir = join(pkg.dir, "src");
  if (!existsSync(sourceDir)) return;
  for (const sourceFile of findTsFiles(sourceDir)) {
    const relativePath = normalizePath(relative(root, sourceFile));
    if (relativePath.includes("/smoke-")) continue;
    if (relativePath.endsWith("/test-support.ts")) continue;
    const source = readFileSync(sourceFile, "utf8");
    for (const dependency of forbidden) {
      if (importsPackage(source, dependency)) {
        violations.push(
          `${packageName} source ${relativePath} must not import ${dependency}`,
        );
      }
    }
  }
}

function findTsFiles(dir) {
  const files = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      files.push(...findTsFiles(path));
    } else if (entry.endsWith(".ts")) {
      files.push(path);
    }
  }
  return files.sort();
}

function importsPackage(source, packageName) {
  const escaped = packageName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(
    String.raw`\b(?:from|import)\s*(?:\([^)]*)?["']${escaped}(?:/[^"']*)?["']`,
  ).test(source);
}

function normalizePath(path) {
  return path.split("\\").join("/");
}
