export type ToolProfilePromptSurfaceClassification =
  | "execution_wrapper"
  | "prompt_renderer"
  | "adapter_glue"
  | "provider_client_implementation"
  | "diagnostic_estimator"
  | "temporary_policy_facade";

export type RemainingPolicyDisposition = "intentional" | "follow_up";

export interface RemainingPolicyNote {
  disposition: RemainingPolicyDisposition;
  note: string;
  followUpTaskId?: number;
}

export interface ToolProfilePromptSurfaceClassificationRecord {
  path: string;
  classification: ToolProfilePromptSurfaceClassification;
  allowedTypeScriptAuthority: string;
  requiredRustBoundary: string;
  remainingPolicy?: RemainingPolicyNote;
}

export const toolProfilePromptSurfaceClassifications: readonly ToolProfilePromptSurfaceClassificationRecord[] =
  [
  {
    path: "local-code-tools.ts",
    classification: "execution_wrapper",
    allowedTypeScriptAuthority:
      "Expose model-callable filesystem, shell, git, and local patch bindings; resolve paths and enforce tool-local process/output bounds.",
    requiredRustBoundary:
      "Tool availability and session resource limits must arrive through Rust-planned profile/session state. Durable lifecycle or tool-profile policy must not be decided here.",
  },
  {
    path: "patch-tool.ts",
    classification: "execution_wrapper",
    allowedTypeScriptAuthority:
      "Apply bounded replace or V4A patches after the tool has been selected for the session.",
    requiredRustBoundary:
      "Tool selection is Rust-planned. Workdir-scoped worker variants must be explicit tool identities, not hidden defaults for full agents.",
  },
  {
    path: "web-tools.ts",
    classification: "provider_client_implementation",
    allowedTypeScriptAuthority:
      "Call search/fetch providers and enforce adapter-local network safety around redirects, private-network access, extraction size, and result formatting.",
    requiredRustBoundary:
      "Rust owns whether the web tools are available to a profile/session. Web results are transient tool output, not coordination state.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Network safety lives with the JS fetch/client adapter because it depends on Node/browser-facing URL and DNS behavior.",
    },
  },
  {
    path: "browser-tools.ts",
    classification: "execution_wrapper",
    allowedTypeScriptAuthority:
      "Expose browser navigation, snapshot, console, action, and screenshot operations against an adapter-owned browser session.",
    requiredRustBoundary:
      "Rust owns tool availability and Crew session lifecycle. Browser actions must remain model-callable effects, not Crew coordination decisions.",
  },
  {
    path: "browser-session-manager.ts",
    classification: "provider_client_implementation",
    allowedTypeScriptAuthority:
      "Manage external browser processes, CDP handles, browser refs, adapter-local cleanup, and diagnostics.",
    requiredRustBoundary:
      "Crew session/archive/restart authority stays in Rust; browser process lifecycle is an adapter resource tied to those facts.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Browser process/resource limits are adapter-local safety limits around an external client implementation, not durable Crew lifecycle policy.",
    },
  },
  {
    path: "skills-tools.ts",
    classification: "temporary_policy_facade",
    allowedTypeScriptAuthority:
      "List/view configured skills and perform explicit skill-management filesystem actions when profile or curator mode enables them.",
    requiredRustBoundary:
      "Profile skill visibility comes from profile config. Any future durable skill governance or auto-mutation policy must move behind Rust planning before it mutates Crew-owned memory/profile state.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Current skill management mutates filesystem skill assets only and is gated by explicit mode flags; durable memory governance is covered by the memory/curator Rust-authority track.",
    },
  },
  {
    path: "mcp-brain-tools.ts",
    classification: "adapter_glue",
    allowedTypeScriptAuthority:
      "Discover MCP tools, normalize model-facing names/arguments, call the configured MCP executor, and map results to brain tool output.",
    requiredRustBoundary:
      "MCP bindings, availability, and catalog policy are Rust-planned runtime facts. TS argument pruning is compatibility glue for optional MCP schema quirks.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Optional-argument pruning is adapter compatibility behavior at the MCP boundary, not a profile/tool availability decision.",
    },
  },
  {
    path: "profile-loading.ts",
    classification: "temporary_policy_facade",
    allowedTypeScriptAuthority:
      "Load profile JSON/YAML, Markdown assets, and skill files; map loaded config into Rust validation/planning inputs and fail closed on Rust diagnostics.",
    requiredRustBoundary:
      "Runtime-affecting profile graph fields, MCP bindings, context policy, channel defaults, session defaults, and tool availability must be validated or planned by Rust.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Prompt assets and skill bodies remain filesystem/Markdown loading concerns; runtime graph fields are checked against Rust-owned config planning smokes.",
    },
  },
  {
    path: "profile-role-assembly.ts",
    classification: "prompt_renderer",
    allowedTypeScriptAuthority:
      "Render profile soul, memory context, skills, tool inventory, planning context, and runtime notes into model-facing prompt text.",
    requiredRustBoundary:
      "Memory/session context selection, runtime profile facts, and tool availability must be preselected by Rust or storage-backed native operations before rendering.",
  },
  {
    path: "context-strategy.ts",
    classification: "temporary_policy_facade",
    allowedTypeScriptAuthority:
      "Publish the UI/provider-facing strategy catalog, normalize strategy patches, and render strategy-specific prompt instructions.",
    requiredRustBoundary:
      "Profile context policy values are validated by core-config. Any strategy that changes wake selection, compaction persistence, or lifecycle behavior must add Rust planning first.",
    remainingPolicy: {
      disposition: "intentional",
      note: "Current active strategy only changes prompt assembly; future runtime-affecting strategies are required to cross Rust planning before activation.",
    },
  },
  {
    path: "context-estimate.ts",
    classification: "diagnostic_estimator",
    allowedTypeScriptAuthority:
      "Produce approximate token/context diagnostics from sampled text and provider budget metadata.",
    requiredRustBoundary:
      "Estimates may inform UI/debug displays. Hard wake refusal, compaction persistence, or strategy transitions must use Rust-owned policy inputs.",
    remainingPolicy: {
      disposition: "intentional",
      note: "The estimator is approximate by design and is not a durable enforcement boundary.",
    },
  },
  {
    path: "delegated-role-assembly.ts",
    classification: "prompt_renderer",
    allowedTypeScriptAuthority:
      "Render role-specific delegated worker prompt text and initial model-facing message from a Rust-accepted lifecycle plan.",
    requiredRustBoundary:
      "Parent/child identity, lineage, resource inheritance, delegation depth, duration ceilings, correlation id, and tool profile facts must come from Rust lifecycle planning.",
  },
  ] as const;

export const requiredToolProfilePromptSurfacePaths = [
  "local-code-tools.ts",
  "patch-tool.ts",
  "web-tools.ts",
  "browser-tools.ts",
  "browser-session-manager.ts",
  "skills-tools.ts",
  "mcp-brain-tools.ts",
  "profile-loading.ts",
  "profile-role-assembly.ts",
  "context-strategy.ts",
  "context-estimate.ts",
  "delegated-role-assembly.ts",
] as const;
