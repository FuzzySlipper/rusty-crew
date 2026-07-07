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
const legacyBrainIslandSrcSmokeCount = 134;
const legacySrcSmokeAllowedImports = new Map([
  [
    normalizePath("ts/packages/brain-island/src/smoke-adapter-diagnostics.ts"),
    new Set(["@rusty-crew/adapter-den", "@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-channel-readback-tool.ts",
    ),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-coordination-tools-live.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-delegated-slice.ts"),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-den-assignment-evidence-e2e.ts",
    ),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-den-channels-e2e.ts"),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-den-memory-tools.ts"),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-den-successor-service.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-admin-profile-create-concurrency.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-pi-agent-rust-bridge.ts"),
    new Set(),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-brain-module-registry.ts",
    ),
    new Set(),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-mcp-reload.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-mcp-surfaces-e2e.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-mcp-tool-registry.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-mcp-tool-telemetry.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-memory-skills-wake.ts"),
    new Set(["@rusty-crew/adapter-den"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-new-session-config-transaction.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-reload-mcp-control.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-responses-concurrency-capacity.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-responses-event-loop.ts"),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-responses-service-field-test.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-roleplay-browser-api.ts"),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-runtime-rebuild-replacement.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-rusty-view-chat-context.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-rusty-view-chat-read-api.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-service-postgres-startup.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
  [
    normalizePath("ts/packages/brain-island/src/smoke-slash-command-e2e.ts"),
    new Set(["@rusty-crew/adapter-mcp"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-telegram-service-connector.ts",
    ),
    new Set(["@rusty-crew/adapter-den", "@rusty-crew/adapter-telegram"]),
  ],
  [
    normalizePath(
      "ts/packages/brain-island/src/smoke-wake-timeout-config-patch.ts",
    ),
    new Set(["@rusty-crew/service-host"]),
  ],
]);

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
expectNoNewSrcSmokes(
  "@rusty-crew/brain-island",
  legacyBrainIslandSrcSmokeCount,
);
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
  for (const sourceFile of sourceAndSmokeFiles(pkg)) {
    const relativePath = normalizePath(relative(root, sourceFile));
    if (relativePath.endsWith("/test-support.ts")) continue;
    const source = readFileSync(sourceFile, "utf8");
    for (const dependency of forbidden) {
      if (isAllowedLegacySmokeImport(relativePath, dependency)) {
        continue;
      }
      if (importsPackage(source, dependency)) {
        const label = relativePath.includes("/smokes/") ? "smoke" : "source";
        violations.push(
          `${packageName} ${label} ${relativePath} must not import ${dependency}`,
        );
      }
    }
  }
}

function sourceAndSmokeFiles(pkg) {
  const roots = [join(pkg.dir, "src"), join(pkg.dir, "smokes")];
  return roots.flatMap((dir) => (existsSync(dir) ? findTsFiles(dir) : []));
}

function expectNoNewSrcSmokes(packageName, expectedCount) {
  const pkg = packagesByName.get(packageName);
  if (!pkg) return;
  const sourceDir = join(pkg.dir, "src");
  if (!existsSync(sourceDir)) return;
  const srcSmokeFiles = findTsFiles(sourceDir)
    .map((sourceFile) => normalizePath(relative(root, sourceFile)))
    .filter((relativePath) => relativePath.includes("/smoke-"));
  if (srcSmokeFiles.length > expectedCount) {
    violations.push(
      `${packageName} has ${srcSmokeFiles.length} src smoke files; move new smokes to ts/packages/<package>/smokes/ and keep the legacy ceiling at ${expectedCount}`,
    );
  }
}

function isAllowedLegacySmokeImport(relativePath, dependency) {
  return (
    legacySrcSmokeAllowedImports.get(relativePath)?.has(dependency) === true
  );
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
