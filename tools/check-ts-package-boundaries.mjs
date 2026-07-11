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
const externalRuntimePackage = "@rusty-crew/external-runtime-codex";
const internalPackagesOutsideExternalRuntime = [
  "@rusty-crew/brain-island",
  "@rusty-crew/contracts",
  "@rusty-crew/native-bridge",
  "@rusty-crew/service-host",
  ...adapterPackages,
];
const adapterAuthorityAllowedCalls = new Map([
  [
    normalizePath("ts/packages/adapter-den/src/index.ts"),
    new Set(["injectDenDataUpdate", "injectExternalEvent"]),
  ],
  [
    normalizePath("ts/packages/adapter-den/src/channel-ingress.ts"),
    new Set([
      "ensureSessionForRoute",
      "injectExternalEvent",
      "routeAgentMessage",
    ]),
  ],
  [
    normalizePath("ts/packages/adapter-den/src/den-product-ingress.ts"),
    new Set(["injectDenDataUpdate", "planDenProductIngressPolicy"]),
  ],
]);
const adapterAuthorityForbiddenCalls = [
  "archiveSession",
  "cancelDelegatedSession",
  "cleanupDelegatedResources",
  "createSession",
  "destroyProfile",
  "drainDelegatedSessions",
  "injectDenDataUpdate",
  "injectExternalEvent",
  "planNewSessionControl",
  "planProfileRegistryMutation",
  "planReloadMcpControl",
  "planRuntimeConfig",
  "registerBrainImplementation",
  "requestBrainWake",
  "requestDelegatedCheckpoint",
  "routeAgentMessage",
  "saveRuntimeConfig",
  "shutdownEngine",
  "submitBrainEvent",
  "submitBrainTextDelta",
  "wakeBrainFromBridgeRequest",
];
const legacyBrainIslandSrcSmokeCount = 0;
const forbiddenProductionResidueNamePattern =
  /(?:^|[-_])(legacy|fallback|compat|shim|scaffold|placeholder|deterministic)(?:[-_.]|$)/i;
const productionResidueFilenameAllowlist = new Set([]);
const relocatedSmokePathRatchets = [
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-admin-profile-create-concurrency.ts",
    ),
    newPath: normalizePath(
      "ts/packages/service-host/smokes/admin-profile-create-concurrency.ts",
    ),
  },
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-new-session-config-transaction.ts",
    ),
    newPath: normalizePath(
      "ts/packages/service-host/smokes/new-session-config-transaction.ts",
    ),
  },
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-mcp-tool-registry.ts",
    ),
    newPath: normalizePath("ts/smokes/mcp-tool-registry.ts"),
  },
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-tool-registry-parity.ts",
    ),
    newPath: normalizePath(
      "ts/packages/brain-island/smokes/tool-registry-parity.ts",
    ),
  },
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-tool-registry-diagnostics.ts",
    ),
    newPath: normalizePath(
      "ts/packages/brain-island/smokes/tool-registry-diagnostics.ts",
    ),
  },
  {
    oldPath: normalizePath(
      "ts/packages/brain-island/src/smoke-local-tool-profile-policy.ts",
    ),
    newPath: normalizePath(
      "ts/packages/brain-island/smokes/local-tool-profile-policy.ts",
    ),
  },
];
expectNoDependencies("@rusty-crew/brain-island", [
  "@rusty-crew/service-host",
  ...adapterPackages,
]);
expectNoDependencies(
  externalRuntimePackage,
  internalPackagesOutsideExternalRuntime,
);
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
expectNoSourceImports(
  externalRuntimePackage,
  internalPackagesOutsideExternalRuntime,
);
expectBrainIslandCompositionRatchets();
expectNoProductionResidueFilenames("@rusty-crew/brain-island");
expectNoNewSrcSmokes(
  "@rusty-crew/brain-island",
  legacyBrainIslandSrcSmokeCount,
);
expectRelocatedSmokesStayMoved();
for (const adapterName of adapterPackages) {
  expectNoSourceImports(adapterName, [
    "@rusty-crew/brain-island",
    "@rusty-crew/service-host",
    ...adapterPackages.filter((candidate) => candidate !== adapterName),
  ]);
}
expectAdapterAuthorityRatchets();

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
      externalRuntimeBoundary: {
        package: externalRuntimePackage,
        forbiddenInternalDependencies: internalPackagesOutsideExternalRuntime,
        shippingTransport: "attached-unix-websocket",
        stdioFallback: false,
      },
      adapterAuthorityRatchet: {
        packages: adapterPackages,
        forbiddenCalls: adapterAuthorityForbiddenCalls.length,
        exactExemptions: [...adapterAuthorityAllowedCalls].map(
          ([path, calls]) => ({
            path,
            calls: [...calls].sort(),
          }),
        ),
      },
      smokeRelocationRatchet: {
        movedFiles: relocatedSmokePathRatchets.length,
        legacySrcSmokeCeiling: legacyBrainIslandSrcSmokeCount,
      },
      productionResidueFilenameRatchet: {
        package: "@rusty-crew/brain-island",
        forbiddenTerms: [
          "legacy",
          "fallback",
          "compat",
          "shim",
          "scaffold",
          "placeholder",
          "deterministic",
        ],
        allowlist: [...productionResidueFilenameAllowlist].sort(),
      },
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
      if (importsPackage(source, dependency)) {
        const label = relativePath.includes("/smokes/") ? "smoke" : "source";
        violations.push(
          `${packageName} ${label} ${relativePath} must not import ${dependency}`,
        );
      }
    }
  }
}

function expectBrainIslandCompositionRatchets() {
  const pkg = packagesByName.get("@rusty-crew/brain-island");
  if (!pkg) return;
  const deletedBrainModulePath = join(pkg.dir, "src", "brain-module.ts");
  if (existsSync(deletedBrainModulePath)) {
    violations.push(
      "@rusty-crew/brain-island must not restore production brain-module.ts; Rust owns the built-in brain catalog and run dispatch",
    );
  }
  const deletedLocalBrainPath = join(pkg.dir, "src", "local-brain.ts");
  if (existsSync(deletedLocalBrainPath)) {
    violations.push(
      "@rusty-crew/brain-island must not restore production local-brain.ts; neutral wake callbacks belong in brain-host-runtime.ts and deterministic brains belong in test support",
    );
  }
  const forbiddenBrainHostAuthority = [
    {
      pattern: /\bBrainModule\b/,
      description: "the legacy BrainModule abstraction",
    },
    {
      pattern: /\bBrainImplementation\b/,
      description: "the ambiguous TypeScript BrainImplementation abstraction",
    },
    {
      pattern: /\bBrainModuleRegistry\b|\bdefaultBrainModules\b/,
      description: "a TypeScript built-in brain registry",
    },
    {
      pattern: /RUSTY_CREW_OPENAI_RESPONSES_LIVE|RUSTY_CREW_PI_AGENT_LIVE/,
      description: "a production fake/live brain switch",
    },
    {
      pattern: /\bmode\s*:\s*["']fake["']/,
      description: "a production fake brain client",
    },
    {
      pattern:
        /\b(?:runOpenAiResponsesBrain|startOpenAiResponsesBrain|drainOpenAiResponsesBrainStream|submitOpenAiResponsesToolOutput|cancelOpenAiResponsesBrain|startPiAgentBrain|drainPiAgentBrainStream|submitPiAgentToolOutput|cancelPiAgentBrain)\b/,
      description: "a provider-specific public brain bridge operation",
    },
  ];
  const allowedTimerFiles = new Set([
    normalizePath("ts/packages/brain-island/src/service-app.ts"),
    normalizePath("ts/packages/brain-island/src/service-adapter-lifecycle.ts"),
    normalizePath("ts/packages/brain-island/src/service-chat-stream-routes.ts"),
  ]);
  const routeTablePath = join(pkg.dir, "src", "service-route-table.ts");
  if (!existsSync(routeTablePath)) {
    violations.push(
      "@rusty-crew/brain-island must keep service route composition in service-route-table.ts",
    );
  }
  const serviceAppPath = join(pkg.dir, "src", "service-app.ts");
  if (existsSync(serviceAppPath)) {
    const serviceAppSource = readFileSync(serviceAppPath, "utf8");
    if (!serviceAppSource.includes("matchServiceApiRoute")) {
      violations.push(
        "brain-island service-app.ts must dispatch through matchServiceApiRoute rather than growing ad hoc route composition",
      );
    }
  }
  for (const sourceFile of productionSourceFiles(pkg)) {
    const relativePath = normalizePath(relative(root, sourceFile));
    const source = readFileSync(sourceFile, "utf8");
    for (const rule of forbiddenBrainHostAuthority) {
      if (!rule.pattern.test(source)) continue;
      violations.push(
        `brain-island production file ${relativePath} must not own ${rule.description}`,
      );
    }
    if (/\bcreateServer\s*\(/.test(source)) {
      violations.push(
        `brain-island production file ${relativePath} must not create HTTP servers; service-host owns process HTTP composition`,
      );
    }
    if (
      /\bsetInterval\s*\(/.test(source) &&
      !allowedTimerFiles.has(relativePath)
    ) {
      violations.push(
        `brain-island production file ${relativePath} must not own process timers; expose a host port or document a boundary ratchet exception`,
      );
    }
    if (/\bstartServiceHostBackgroundLoopTimers\b/.test(source)) {
      violations.push(
        `brain-island production file ${relativePath} must not start service-host background loop timers`,
      );
    }
  }
}

function expectNoProductionResidueFilenames(packageName) {
  const pkg = packagesByName.get(packageName);
  if (!pkg) return;
  for (const sourceFile of productionSourceFiles(pkg)) {
    const relativePath = normalizePath(relative(root, sourceFile));
    const fileName = relativePath.split("/").at(-1) ?? relativePath;
    if (!forbiddenProductionResidueNamePattern.test(fileName)) continue;
    if (productionResidueFilenameAllowlist.has(relativePath)) continue;
    violations.push(
      `${packageName} production file ${relativePath} looks like legacy/fallback/scaffold residue; move it to smokes/test support, rename it to its durable role, or add an explicit reviewed allowlist entry with a Den task`,
    );
  }
}

function expectAdapterAuthorityRatchets() {
  for (const adapterName of adapterPackages) {
    const pkg = packagesByName.get(adapterName);
    if (!pkg) continue;
    for (const sourceFile of productionSourceFiles(pkg)) {
      const relativePath = normalizePath(relative(root, sourceFile));
      const source = readFileSync(sourceFile, "utf8");
      for (const callName of adapterAuthorityForbiddenCalls) {
        if (!usesIdentifier(source, callName)) continue;
        if (adapterAuthorityAllowedCalls.get(relativePath)?.has(callName)) {
          continue;
        }
        violations.push(
          `${adapterName} production file ${relativePath} must not call or expose ${callName}; adapters may normalize, project, diagnose, or use exact approved planner/ingress ports only`,
        );
      }
    }
  }
}

function productionSourceFiles(pkg) {
  const sourceDir = join(pkg.dir, "src");
  if (!existsSync(sourceDir)) return [];
  return findTsFiles(sourceDir).filter((sourceFile) => {
    const relativePath = normalizePath(relative(root, sourceFile));
    return (
      !relativePath.includes("/smoke-") &&
      !relativePath.endsWith("/test-support.ts")
    );
  });
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
      `${packageName} has ${srcSmokeFiles.length} src smoke files; move smokes to ts/packages/<package>/smokes/ or ts/smokes/ instead of production src`,
    );
  }
}

function expectRelocatedSmokesStayMoved() {
  for (const { oldPath, newPath } of relocatedSmokePathRatchets) {
    if (existsSync(join(root, oldPath))) {
      violations.push(
        `relocated smoke ${oldPath} must stay moved to ${newPath}`,
      );
    }
    if (!existsSync(join(root, newPath))) {
      violations.push(
        `relocated smoke ratchet expects ${newPath}; update the ratchet only with the relocation commit that moves it again`,
      );
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

function usesIdentifier(source, identifier) {
  const escaped = identifier.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(String.raw`\b${escaped}\b`).test(source);
}

function normalizePath(path) {
  return path.split("\\").join("/");
}
