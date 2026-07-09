# TypeScript Tool/Profile/Prompt Surface Inventory

Date: 2026-07-08
Task: 4738

This document classifies the remaining TypeScript surface in the
tool/profile/prompt cluster after the first Rust-authority thinning pass. It is
a boundary inventory, not a new architecture proposal.

The rule of thumb is:

- Rust owns durable profile/runtime validation, tool availability policy,
  session lifecycle, delegation lineage, resource-limit inheritance, and
  runtime-affecting context policy.
- TypeScript may own executable tool bindings, external client adapters,
  Markdown/profile asset loading, provider/client glue, diagnostics projection,
  and prompt rendering.
- TypeScript must not add new lifecycle, selection, storage, or durable policy
  without calling a Rust/native planner or recording a follow-up migration task.

## Current Rust Boundaries

| Boundary | Rust owner | TypeScript caller |
| --- | --- | --- |
| Runtime/profile config validation and create-profile planning | `crates/core/core-config` through generated core-config facade/native bridge | `profile-loading.ts`, `runtime-config-validation.ts`, service config/profile routes |
| Tool availability and metadata policy | `crates/core/core-tool-registry` through native planning | `profile-loading.ts`, tool registry/profile selection composition |
| Delegated role lifecycle, lineage, and resource inheritance | `plan_delegated_role_lifecycle` in `core-config` through the native bridge | `delegated-role-assembly.ts`, production delegation wake smoke |
| Context policy validation | `ProfileContextPolicy` in `core-config` | `profile-loading.ts`, `context-strategy.ts`, context strategy routes |
| Session/resource defaults | `core-config` validation plus Rust session state | local code, browser, web, and patch tool contexts receive already-selected resource facts |

## Production TypeScript Classification

| File | Classification | Allowed TypeScript authority | Required Rust boundary |
| --- | --- | --- | --- |
| `local-code-tools.ts` | `execution_wrapper` | Expose model-callable filesystem, shell, git, and local patch bindings; resolve paths and enforce tool-local process/output bounds. | Tool availability and session resource limits must arrive through Rust-planned profile/session state. Durable lifecycle or tool-profile policy must not be decided here. |
| `patch-tool.ts` | `execution_wrapper` | Apply bounded replace or V4A patches after the tool has been selected for the session. | Tool selection is Rust-planned. Workdir-scoped worker variants must be explicit tool identities, not hidden defaults for full agents. |
| `web-tools.ts` | `provider_client_implementation` | Call search/fetch providers and enforce adapter-local network safety around redirects, private-network access, extraction size, and result formatting. | Rust owns whether the web tools are available to a profile/session. Web results are transient tool output, not coordination state. |
| `browser-tools.ts` | `execution_wrapper` | Expose browser navigation, snapshot, console, action, and screenshot operations against an adapter-owned browser session. | Rust owns tool availability and Crew session lifecycle. Browser actions must remain model-callable effects, not Crew coordination decisions. |
| `browser-session-manager.ts` | `provider_client_implementation` | Manage external browser processes, CDP handles, browser refs, adapter-local cleanup, and diagnostics. | Crew session/archive/restart authority stays in Rust; browser process lifecycle is an adapter resource tied to those facts. |
| `skills-tools.ts` | `temporary_policy_facade` | List/view configured skills and perform explicit skill-management filesystem actions when profile or curator mode enables them. | Profile skill visibility comes from profile config. Any future durable skill governance or auto-mutation policy must move behind Rust planning before it mutates Crew-owned memory/profile state. |
| `mcp-brain-tools.ts` | `adapter_glue` | Discover MCP tools, normalize model-facing names/arguments, call the configured MCP executor, and map results to brain tool output. | MCP bindings, availability, and catalog policy are Rust-planned runtime facts. TS argument pruning is compatibility glue for optional MCP schema quirks. |
| `profile-loading.ts` | `temporary_policy_facade` | Load profile JSON/YAML, Markdown assets, and skill files; map loaded config into Rust validation/planning inputs and fail closed on Rust diagnostics. | Runtime-affecting profile graph fields, MCP bindings, context policy, channel defaults, session defaults, and tool availability must be validated or planned by Rust. |
| `profile-role-assembly.ts` | `prompt_renderer` | Render profile soul, memory context, skills, tool inventory, planning context, and runtime notes into model-facing prompt text. | Memory/session context selection, runtime profile facts, and tool availability must be preselected by Rust or storage-backed native operations before rendering. |
| `context-strategy.ts` | `temporary_policy_facade` | Publish the UI/provider-facing strategy catalog, normalize strategy patches, and render strategy-specific prompt instructions. | Profile context policy values are validated by core-config. Any strategy that changes wake selection, compaction persistence, or lifecycle behavior must add Rust planning first. |
| `context-estimate.ts` | `diagnostic_estimator` | Produce approximate token/context diagnostics from sampled text and provider budget metadata. | Estimates may inform UI/debug displays. Hard wake refusal, compaction persistence, or strategy transitions must use Rust-owned policy inputs. |
| `delegated-role-assembly.ts` | `prompt_renderer` | Render role-specific delegated worker prompt text and initial model-facing message from a Rust-accepted lifecycle plan. | Parent/child identity, lineage, resource inheritance, delegation depth, duration ceilings, correlation id, and tool profile facts must come from Rust lifecycle planning. |

## Remaining Policy Owners

The following TypeScript surfaces still contain policy-adjacent logic. They are
allowed for now only under the listed constraint.

| Surface | Current policy risk | Disposition |
| --- | --- | --- |
| `web-tools.ts` | Network safety policy is enforced in TypeScript alongside Node fetch/DNS behavior. | Intentional: this is adapter-local client safety. Rust decides exposure; the JS adapter decides whether a URL/fetch is safe for the configured client. |
| `browser-session-manager.ts` | Browser process/session limits look like lifecycle policy. | Intentional: these are adapter-local external process limits tied to Rust session facts. Crew session lifecycle remains Rust-owned. |
| `skills-tools.ts` | Skill management can create, patch, write, or archive local skill files. | Intentional while scoped to filesystem skill assets and explicit mode flags. Durable memory/profile governance must use the memory/curator Rust-authority track before becoming auto-mutating policy. |
| `mcp-brain-tools.ts` | Optional-argument pruning changes outgoing MCP tool arguments. | Intentional adapter compatibility at the MCP boundary; it must not become tool availability or profile policy. |
| `profile-loading.ts` | Profile loading still normalizes prompt assets, skill lists, and some config source formats. | Intentional: prompt assets and skill bodies are filesystem/Markdown concerns. Runtime graph fields must remain covered by Rust config validation smokes. |
| `context-strategy.ts` | Strategy catalog/defaults and prompt instructions live in TS. | Intentional while active strategies only change prompt assembly. Runtime-affecting strategies must add Rust planning before activation. |
| `context-estimate.ts` | Approximate token estimates can be tempting to treat as enforcement. | Intentional diagnostic estimator only; hard limits and compaction transitions require Rust-owned policy inputs. |

## Rules For New Tool/Profile/Prompt TypeScript

1. Classify the file before adding it to this cluster.
2. If it persists, selects, denies, schedules, delegates, archives, mutates
   multi-record state, or enforces runtime ceilings, call a Rust/native planner
   first.
3. If it calls an external SDK, MCP server, browser, web provider, filesystem,
   or shell, keep that client glue in TypeScript and pass only factual state
   into Rust.
4. Prompt rendering may stay in TypeScript, but lifecycle/resource/tool facts
   must be frozen before rendering.
5. Worker-specific restrictions must be explicit tool identities or Rust-planned
   resource facts, not hidden behavior changes in full-agent defaults.

## Validation

```bash
npm run smoke:tool-profile-prompt-authority -w @rusty-crew/brain-island
npm run smoke:profile-loading -w @rusty-crew/brain-island
npm run smoke:profile-role-assembly -w @rusty-crew/brain-island
npm run smoke:delegated-role-assembly -w @rusty-crew/brain-island
npm run smoke:local-code-tools -w @rusty-crew/brain-island
npm run smoke:patch-tool -w @rusty-crew/brain-island
npm run smoke:browser-tools -w @rusty-crew/brain-island
npm run smoke:skills-tools -w @rusty-crew/brain-island
npm run typecheck
```

