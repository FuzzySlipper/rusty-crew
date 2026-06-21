# Tool Architecture And Registry Rules

Status: Accepted design note for task 2853

Date: 2026-06-20

Related decision: `adr/0014-tool-profile-enforcement.md`.

## Context

The authoritative Rusty Crew architecture assigns deterministic coordination to
Rust and non-deterministic capability to TypeScript. Tools sit across that
boundary:

- TypeScript owns model-callable tool definitions, adapters, execution, model
  provider integration, MCP tool conversion, and local code tool
  implementations.
- Rust owns session lifecycle, `ToolProfile` contracts, resource limits,
  durable coordination events, wake/lifecycle authority, and hooks that must
  not be bypassed.

Pi-crew has useful tool implementations and catalog concepts, but it also has
tool policy, worker-pool, MCP, and runtime concerns interleaved in ways Rusty
Crew should not copy directly. Rusty Crew tools must flow through one
centralized registry so duplicate tools do not quietly drift into separate
behaviors.

## Ownership Boundary

### TypeScript Owns

- Model-callable tool metadata and implementation modules.
- Local code tools such as `read_file`, `write_file`, `search_files`,
  `terminal`, `git_status`, `git_diff`, and `patch`.
- Browser, web, memory, skill, MCP, and other adapter-backed tool execution.
- Profile loading, requested toolsets, role assembly, and selected tool
  inventory construction.
- Tool invocation during the brain turn.

Once a tool is selected into a profile and injected into the brain, the model is
allowed to use it. Rusty Crew does not add per-call "are you sure?" gates in the
coordination layer. Dangerous tools are excluded by profile/toolset selection or
bounded by infrastructure/resource constraints.

### Rust Owns

- `ToolProfile` descriptors attached to sessions.
- Resource limits such as workdir, max duration, and delegation depth.
- Session/tool-profile auditability: which tools a session was allowed to see.
- Durable tool execution telemetry carried by brain events or future internal
  tool events.
- Lifecycle hooks for cancellation, timeout, drain, and coordination-state
  validation.
- Rejection of malformed session/tool descriptors at registration boundaries.

Rust does not execute ordinary TypeScript tools. Rust may enforce constraints
that must not be bypassed, such as workdir scope or cancellation state, through
bridge-visible contracts and durable events.

## Canonical Registry

There must be one canonical TypeScript-side registry for model-callable tool
metadata. Future implementation should place it under the brain island or a
dedicated tool package, then export registry/inventory helpers through stable
module paths.

Each tool entry must include:

- `name`: canonical model-callable name.
- `description`: user/model-facing behavior summary.
- `category`: coarse domain such as `local`, `git`, `patch`, `web`, `browser`,
  `memory`, `skills`, `mcp`, `delegation`, `planning`, or `diagnostics`.
- `toolsets`: named sets that profiles request.
- `implementationModule`: stable module path for the executor.
- `surfaces`: where the tool can appear, for example `brain`, `mcp`, `admin`,
  `tui`, or `diagnostic`.
- `safety`: metadata such as read-only, writes-files, executes-process,
  network-access, external-write, or coordination-action.
- `outputShape`: stable result family or schema id.
- `version`: semantic tool contract version.
- `deprecated`: optional deprecation metadata.
- `replacement`: optional canonical replacement name.
- `aliases`: optional legacy names, never another primary tool.
- `inventoryTest`: marker or test reference proving the tool appears in
  registry diagnostics.

Tools may not be injected into a brain by constructing ad hoc per-agent lists.
Profiles and smokes may request toolsets or explicit tool names, but final
selection must be built from registry entries.

## Naming Rules

- Use lower snake case for canonical model-callable names.
- Names should be verb-object or noun-object: `read_file`, `git_diff`,
  `web_extract`, `skill_view`.
- Avoid numbered variants such as `do_thing2`. Add a version field or deprecate
  and replace the old tool.
- Avoid synonyms that hide capability overlap. If two tools claim the same
  output shape and category, the registry diagnostic must flag the collision.
- Adapter-specific names should be prefixed only when needed to avoid ambiguity,
  for example `den_memory_recall`.
- MCP imported tools must keep a stable source identity in metadata even when
  exposed under an unprefixed model name.

## Category And Toolset Rules

Category describes what a tool does. Toolsets describe why a profile receives
it.

Examples:

- `local_code_read`: `read_file`, `search_files`, `git_status`, `git_diff`.
- `local_code_write`: `write_file`, `patch`, `terminal`.
- `review_readonly`: read-only local and web tools.
- `delegation_basic`: action-producing helpers that emit structured
  `BrainAction::RequestDelegation`, not direct TS spawning.
- `memory_den`: Den memory tools backed by the Den adapter.
- `mcp_project`: tools imported from project MCP servers.

Profiles request toolsets or explicit tool names. Session constraints may
remove tools from the final inventory but should explain the reason.

## Selection And Inventory

The registry selection layer must produce an explainable inventory. Every
registered tool considered for a session should have one of these outcomes:

- `selected`
- `not_requested`
- `profile_denied`
- `session_denied`
- `resource_denied`
- `deprecated`
- `missing`
- `shadowed`
- `collision`

The final selected tools become:

1. TypeScript brain tool bindings used during inference.
2. Rust `ToolProfile` descriptors for session audit and wake context.
3. Inventory diagnostics for operators and tests.

The inventory report is the preferred place to debug "why did this agent not
get a tool?" It should include the selected toolsets, profile id, session id,
resource limits, and collision/deprecation explanations.

## Deprecation And Replacement

Renaming a tool is a registry operation, not a search-and-replace.

To replace a tool:

1. Add the new canonical entry.
2. Mark the old entry as deprecated with a replacement.
3. Keep the old name as an alias only when compatibility is required.
4. Add or update inventory diagnostics so both names cannot be selected as
   separate tools.
5. Remove the alias only after profiles and persisted references have migrated.

Deprecated tools should not silently appear in new profiles. Selection must
either deny them with a reason or resolve them to the replacement with explicit
inventory evidence.

## Collision Rules

Registry validation must fail when:

- two active entries share a canonical name;
- an alias matches another active canonical name;
- two active tools claim the same `(category, outputShape)` pair without an
  explicit coexistence note;
- two entries point at the same implementation module with different output
  shapes;
- a deprecated tool has no replacement or sunset note;
- a tool is exported from an implementation module but absent from the registry.

These checks are how Rusty Crew prevents `do_thing` / `do_thing2` drift.

## Rust Hook Rules

A tool needs Rust contract or hook work when it affects coordination state or
must be auditable across process restarts. Examples:

- Delegation tools should emit `BrainAction::RequestDelegation`; they must not
  spawn sessions directly in TypeScript.
- Completion tools should emit `BrainAction::DeliverCompletion`; Rust persists
  packets and schedules parent wakes.
- Tool start/end/error should be visible as durable brain events or future
  typed tool events.
- Workdir, timeout, cancellation, and drain behavior should be enforceable from
  Rust-owned session/resource state.
- Den projection is observability only; failing projection must not block tool
  execution already accepted by Rust.

Ordinary file, shell, web, browser, memory, and MCP tools can remain TypeScript
executors as long as they respect the selected inventory and session resource
limits.

## Pi-Crew Adaptation Rules

Adapt pi-crew behavior by responsibility:

- Local filesystem, shell, git, web, browser, memory, skills, and patch tools
  should be ported as TypeScript tool executors through the canonical registry.
- MCP discovery/conversion should stay TypeScript-side in the MCP adapter and
  feed the same registry/inventory model.
- Delegation and fan-out should expose model-callable intent helpers only if
  useful, but actual session creation, lineage, lifecycle, and completion
  remain Rust-owned actions.
- Worker-pool-specific policy should not become the default full/prime-agent
  tool gate.
- Useful pi-crew guardrails such as SSRF checks, syntax-check rollback, and
  stable output shapes should be preserved in the TypeScript executor modules.

## Implementation Sequence

Future tool tasks should proceed in this order:

1. Build the canonical registry and inventory model.
2. Map registry/profile selection into Rust `ToolProfile` contracts.
3. Load profiles and skills into role assembly.
4. Implement reproducible tool selection and session filtering.
5. Port local read/terminal/git tools, then the `patch` tool.
6. Add Rust-side tool telemetry/hooks.
7. Add diagnostics and duplicate-tool drift checks.
8. Prove read/terminal and patch through production brain wakes.

No later task should depend on worker-pool availability to make ordinary
full/prime-agent tools work.
