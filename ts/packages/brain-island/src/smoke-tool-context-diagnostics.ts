import assert from "node:assert/strict";
import type { ProfileId, SessionId } from "@rusty-crew/contracts";
import {
  buildProfileRoleAssembly,
  buildToolContextDiagnosticsReport,
  buildToolRegistryDiagnostics,
  createToolRegistry,
  formatToolContextDiagnosticsMarkdown,
  selectToolProfile,
} from "./index.js";
import type {
  LoadedProfileContext,
  ToolRegistryEntry,
  ToolRegistryDiagnosticsReport,
} from "./index.js";

const profileId = "diagnostics-profile" as ProfileId;
const sessionId = "diagnostics-session" as SessionId;
const sessionConstraints = {
  readOnly: true,
  deniedTools: ["git_diff"],
};
const policy = {
  requestedToolsets: ["local_code_read", "local_code_write"],
  requestedTools: ["missing_tool"],
  deniedTools: ["terminal"],
};
const selectedRegistry = createToolRegistry([
  entry("read_file", "local.file_text.v1"),
  entry("write_file", "local.file_write_result.v1", ["writes_files"]),
  entry("search_files", "local.file_search_result.v1"),
  entry("terminal", "local.terminal_result.v1", [
    "executes_process",
    "writes_files",
  ]),
  entry("git_diff", "git.diff_result.v1"),
  {
    ...entry("mcp_memory_search", "memory.search_result.v1"),
    category: "mcp",
    toolsets: ["memory"],
    implementationModule:
      "./mcp-tools.js#mcp-memory:binding-memory:memory_search",
  },
] satisfies readonly ToolRegistryEntry[]);
const toolSelection = selectToolProfile({
  profileId,
  policy,
  session: sessionConstraints,
  registry: selectedRegistry,
  catalogId: "diagnostics-catalog",
});
const toolDiagnostics = buildToolRegistryDiagnostics({
  catalogId: "diagnostics-catalog",
  registry: selectedRegistry,
  inventoryRequest: {
    requestedToolsets: policy.requestedToolsets,
    requestedTools: policy.requestedTools,
    profileDeniedTools: policy.deniedTools,
    sessionDeniedTools: sessionConstraints.deniedTools,
    resourceDeniedTools: ["write_file"],
  },
});
const profileContext = {
  profile: {
    profileId,
    displayName: "Diagnostics Profile",
    modelConfig: {
      provider: "local",
      modelName: "deterministic",
    },
    runtime: {
      maxTurns: 4,
      defaultResourceLimits: {
        workdir: "/home/dev/rusty-crew",
        maxDurationMs: 30_000,
        maxDelegationDepth: 2,
      },
    },
    toolPolicy: policy,
    prompt: {
      system: "Use diagnostics without dumping raw prompt content.",
      instructions: ["Explain tool availability.", "Avoid raw secrets."],
    },
    skills: ["diagnostics"],
  },
  skills: [
    {
      slug: "diagnostics",
      title: "Diagnostics",
      summary: "Explain runtime surfaces.",
      tags: ["diagnostics"],
      bodyMarkdown: "Never expose full prompt bodies in diagnostics output.",
      sourcePath: "/tmp/diagnostics.md",
    },
  ],
  toolSelection,
} satisfies LoadedProfileContext;
const assembled = buildProfileRoleAssembly(profileContext);

const report = buildToolContextDiagnosticsReport({
  now: "2026-06-20T00:00:00Z",
  session: {
    sessionId,
    agentId: "diagnostics-agent",
    profileId,
    kind: "prime",
  },
  toolDiagnostics,
  toolSelection,
  profileContext,
  sessionConstraints,
  roleAssembly: assembled.roleAssembly,
  systemPrompt: assembled.systemPrompt,
  memorySkillsPlanning: {
    denMemory: {
      configured: true,
      clientAvailable: false,
      endpointConfigured: true,
      mode: "metadata",
      lastError: "connection refused",
    },
    skills: {
      rootConfigured: true,
      rootReadable: true,
      profileSkillCount: 2,
      loadedSkillCount: 1,
      pinnedSkillCount: 1,
      protectedSkillCount: 1,
      invalidSkillCount: 0,
      missingSkillCount: 1,
    },
    denseProfileMemory: {
      clientAvailable: true,
      recordCount: 4,
      maxRecordsPerProfile: 20,
      capReached: false,
    },
    sessionSearch: {
      available: false,
      indexedRows: 0,
      lastError: "runtime search index unavailable",
    },
    todo: {
      available: true,
      itemCount: 2,
      blockedItems: 1,
    },
    counters: {
      available: true,
      resetAllowed: false,
      summary: {
        wakes: 3,
        toolCalls: 7,
        messages: 11,
      },
    },
  },
  adapters: {
    generatedAt: "2026-06-20T00:00:00Z",
    degraded: true,
    channels: {
      totalBindings: 1,
      activeBindings: 1,
      degradedBindings: 0,
      droppedProjections: 0,
      bindings: [
        {
          bindingId: "channel-main",
          adapterId: "den",
          agentId: "diagnostics-agent",
          sessionId,
          profileId,
          provider: "den",
          status: "active",
          membershipStatus: "joined",
          presenceStatus: "online",
          subscriptionStatus: "active",
          stalePresence: false,
          droppedProjections: 0,
        },
      ],
    },
    mcp: {
      totalSurfaces: 1,
      activeSurfaces: 0,
      degradedSurfaces: 1,
      collisionCount: 1,
      reloadCount: 1,
      surfaces: [
        {
          bindingId: "mcp-memory",
          adapterId: "mcp",
          agentId: "diagnostics-agent",
          sessionId,
          profileId,
          status: "degraded",
          transport: "stdio",
          serverNames: ["memory"],
          toolProfileKey: "diagnostics-profile:memory",
          reconnectAttempts: 2,
          collisionCount: 1,
          discoveryIssueCount: 1,
          optionalServerFailures: ["memory"],
          lastError: "tool name collision during discovery",
        },
      ],
    },
    issues: ["mcp mcp-memory: tool name collision during discovery"],
  },
});

assert.equal(report.summary.selectedTools, 2);
assert.equal(report.summary.missingTools, 1);
assert.equal(report.summary.mcpTools, 1);
assert.equal(
  report.tools.find((tool) => tool.name === "terminal")?.status,
  "denied",
);
assert.deepEqual(
  report.tools.find((tool) => tool.name === "missing_tool")?.reasonCodes,
  ["missing_requested_tool"],
);
assert.equal(report.context.sections.includes("Profile"), true);
assert.equal(report.context.skills[0]?.bodyChars, 54);
assert.equal(report.memorySkillsPlanning.denMemory.clientAvailable, false);
assert.equal(report.memorySkillsPlanning.skills.pinnedSkillCount, 1);
assert.equal(report.memorySkillsPlanning.denseProfileMemory.recordCount, 4);
assert.equal(report.memorySkillsPlanning.todo.itemCount, 2);
assert.equal(
  report.issues.some((issue) => issue.code === "den_memory_unavailable"),
  true,
);
assert.equal(
  report.issues.some((issue) => issue.code === "session_search_unavailable"),
  true,
);
assert.equal(
  "Use diagnostics without dumping raw prompt content.".includes(
    report.context.systemPrompt.sha256 ?? "",
  ),
  false,
);
assert.equal(report.resources.workdirScoped, true);
assert.equal(report.adapters.mcp.degraded, 1);
assert.equal(
  report.issues.some((issue) => issue.code === "mcp_surface_degraded"),
  true,
);

const collisionReport = buildToolContextDiagnosticsReport({
  now: "2026-06-20T00:00:00Z",
  session: {
    sessionId: "collision-session",
    agentId: "collision-agent",
    profileId,
  },
  toolDiagnostics: buildToolRegistryDiagnostics({
    catalogId: "collision-catalog",
    entries: [
      entry("do_thing", "common.result.v1"),
      entry("do_thing2", "common.result.v1"),
    ],
  }) as ToolRegistryDiagnosticsReport,
});
assert.equal(collisionReport.summary.collidedTools, 2);
assert.equal(
  collisionReport.issues.some((issue) => issue.code === "registry_collision"),
  true,
);

const markdown = formatToolContextDiagnosticsMarkdown(report);
assert.match(markdown, /Tool and Context Diagnostics/);
assert.match(markdown, /missing_tool/);
assert.match(markdown, /den memory: unavailable/);
assert.match(markdown, /pinned skills: 1/);
assert.doesNotMatch(
  markdown,
  /Use diagnostics without dumping raw prompt content\./,
);

console.log(
  JSON.stringify(
    {
      summary: report.summary,
      sections: report.context.sections,
      issues: report.issues.map((issue) => issue.code),
      collisionSummary: collisionReport.summary,
      markdownLines: markdown.split("\n").length,
    },
    null,
    2,
  ),
);

function entry(
  name: string,
  outputShape: string,
  safety: ToolRegistryEntry["safety"] = ["read_only"],
): ToolRegistryEntry {
  return {
    name,
    description: `${name} description`,
    category: name.startsWith("git_") ? "git" : "local",
    toolsets: name.startsWith("git_")
      ? ["local_code_read", "review_readonly"]
      : safety.includes("read_only")
        ? ["local_code_read", "review_readonly"]
        : ["local_code_write"],
    implementationModule: `./tools.js#${name}`,
    surfaces: ["brain"],
    safety,
    outputShape,
    version: "1.0.0",
    inventoryTest: "smoke:tool-context-diagnostics",
  };
}
