import assert from "node:assert/strict";

import * as brainIsland from "@rusty-crew/brain-island";
import type {
  AdminApiEnvelope,
  AdminControlExecutor,
  BrainImplementation,
  BrainTool,
  ContextStrategyPolicy,
  DebugApiClient,
  ProfileConfig,
  RoleplayNarratorBrainOptions,
  RustyCrewServiceAppOptions,
  ServiceAdapterFactories,
  ToolRegistry,
} from "@rusty-crew/brain-island";

type RootTypeCompatibility = {
  adminEnvelope: AdminApiEnvelope<unknown>;
  adminExecutor: AdminControlExecutor;
  brain: BrainImplementation;
  contextPolicy: ContextStrategyPolicy;
  debugClient: DebugApiClient;
  profile: ProfileConfig;
  roleplayOptions: RoleplayNarratorBrainOptions;
  serviceAdapters: ServiceAdapterFactories;
  serviceOptions: RustyCrewServiceAppOptions;
  tool: BrainTool;
  registry: ToolRegistry;
};

const runtimeExportGroups: Record<string, string[]> = {
  coreBrain: [
    "createBrainWakeExecutor",
    "registerBrainImplementationRuntime",
    "wakeBrainFromBridgeRequest",
  ],
  serviceConfig: [
    "createRustyCrewServiceApp",
    "loadRustyCrewServiceConfig",
    "validateRustyCrewServiceConfig",
    "acquireRustyCrewServiceLock",
    "ensureRustyCrewServiceDirectories",
  ],
  tools: [
    "readFileTool",
    "writeFileTool",
    "terminalTool",
    "patchTool",
    "delegationTools",
    "completionTools",
    "coordinationTools",
    "defaultToolRegistry",
    "createToolRegistry",
  ],
  memoryRoleplayContext: [
    "createDenMemoryToolResolver",
    "createDenseProfileMemoryToolResolver",
    "createLoreMemoryToolResolver",
    "createRoleplayNarratorBrain",
    "createRoleplayNarratorFsmBridge",
    "buildProfileRoleAssembly",
    "createMemorySpaceToolResolver",
    "contextStrategyCatalog",
    "estimateContextUsage",
    "evaluateContextCompactionTrigger",
  ],
  diagnosticsObservation: [
    "createAgentActivityObservationEvent",
    "createRuntimeActivityObserver",
    "buildRuntimeDiagnosticsProjection",
    "buildRuntimeHealthProjection",
    "buildAdapterDiagnosticsProjection",
    "buildToolRegistryDiagnostics",
    "buildToolContextDiagnosticsReport",
  ],
  adminCommandsProfiles: [
    "handleAdminDiagnosticsRequest",
    "handleAdminControlRequest",
    "routeSlashCommand",
    "buildReadOnlySlashCommandResponse",
    "createNewSessionLifecycleExecutor",
    "createReloadMcpControlExecutor",
    "loadProfileConfig",
    "buildProfileRegistryImportPlan",
    "buildAdminProfileRegistryDiagnostics",
    "buildProfileBundleExportPlan",
    "planCreateProfileWithRust",
  ],
  browserDebugMcp: [
    "createDebugApiClient",
    "inspectDirectDebugSession",
    "BrowserSessionManager",
    "createBrowserToolResolver",
    "reloadMcpSurface",
    "integrateMcpToolsWithRegistry",
    "createMcpToolCallMetadata",
  ],
  backgroundSchedulerCurator: [
    "executeScheduledHostRun",
    "runScheduledHostExecutors",
    "createBackgroundAdminControlExecutor",
    "publishBackgroundGovernanceObservation",
    "runBackgroundMemorySkillReview",
    "discoverCuratorCandidates",
    "runCuratorLifecycleTransitions",
    "createCuratorGovernanceExecutor",
  ],
};

for (const [group, names] of Object.entries(runtimeExportGroups)) {
  for (const name of names) {
    assert.notEqual(
      brainIsland[name as keyof typeof brainIsland],
      undefined,
      `${group} export ${name} should stay available from @rusty-crew/brain-island`,
    );
  }
}

assert.equal(
  typeof brainIsland.createRustyCrewServiceApp,
  "function",
  "service app factory should stay callable from the package root",
);
for (const testOnlyExport of [
  "createLocalBrain",
  "createPlaceholderBrain",
  "envelope",
]) {
  assert.equal(
    testOnlyExport in brainIsland,
    false,
    `${testOnlyExport} must remain test-only`,
  );
}
assert.equal(
  typeof brainIsland.readFileTool,
  "function",
  "local code tools should stay callable from the package root",
);
assert.equal(
  typeof brainIsland.RUSTY_CREW_DEFAULT_ADMIN_HOST,
  "string",
  "service config constants should stay available from the package root",
);
assert.equal(
  typeof brainIsland.API_CAPABILITIES,
  "object",
  "API capability registry data should stay available from the package root",
);
assert.equal(
  typeof brainIsland.SLASH_COMMAND_REGISTRY,
  "object",
  "slash command registry data should stay available from the package root",
);
assert.equal(
  typeof brainIsland.ToolRegistry,
  "function",
  "ToolRegistry class should stay available from the package root",
);

void (undefined as unknown as RootTypeCompatibility | undefined);

console.log("brain-island entrypoint surface smoke passed");
