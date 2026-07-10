# ADR 0023: Rust Brain Catalog And TypeScript Host Capabilities

Status: Implemented by task 5389

Date: 2026-07-09

Supersedes: ADR 0021 where it treats TypeScript brain modules as a permanent
first-class production path.

## Context

Rusty Crew now has two production provider loops in Rust:

- `crates/brains/openai-responses`;
- `crates/brains/pi-agent`.

Both use the neutral wake, stream, action, provider-state, and tool-call
contracts. `crates/brains/brain-runtime` already owns the shared buffered-run
records and registry. The native bridge owns one registry per service instance,
so active runs no longer depend on a process-global fallback.

Before the clean-break cutover, production brain authority was concentrated in
`ts/packages/brain-island/src/brain-module.ts`. It owned:

- the built-in brain catalog and aliases;
- module and strategy selection;
- strategy/provider-state metadata and rebuild policy;
- provider-state fingerprint options;
- provider-specific run input construction;
- the shared incremental drain loop;
- neutral tool lookup and execution;
- tool output truncation and repeated/consecutive failure policy;
- tool debug-reference correlation;
- OpenAI credential update projection;
- roleplay narrator phase execution;
- deterministic local/fake brain behavior.

That was an attractive place for future TypeScript policy to accumulate. A
smaller `brain-module.ts` would reduce the immediate file size but preserve the
wrong long-term extension point.

## Decision

Rusty Crew will delete `brain-module.ts` as a production abstraction.

Rust owns the canonical built-in brain catalog, selection, strategy metadata,
provider-state policy, buffered turn state machine, and all production brain
loops. TypeScript exposes only narrowly named host capabilities required by
tool and adapter implementations that genuinely execute in JavaScript.

The production boundary is:

```text
Rust brain catalog + selection plan
  -> Rust brain run host + provider loop
    -> neutral host capability request
      -> TypeScript tool/adapter executor
    <- neutral host capability result
  <- neutral stream/actions/provider state
```

The boundary is not:

```text
TypeScript brain registry
  -> provider-specific TypeScript brain wrapper
    -> Rust provider loop
```

No compatibility registry or legacy fallback remains after cutover.

The landed TypeScript host boundary is deliberately split by responsibility:

- `brain-host-runtime.ts`: neutral wake callback and Rust registration adapter;
- `buffered-brain-host.ts`: generic drain/submit loop over Rust directives;
- `tool-execution-host.ts`: concrete tool lookup and execution only;
- `provider-debug-projection.ts`: bounded non-authoritative debug projection;
- `pi-agent-host.ts` and `openai-responses-host.ts`: provider input and client
  adaptation without provider-loop lifecycle policy;
- `built-in-brain-host.ts`: exhaustive dispatch of Rust-selected canonical ids
  to those host adapters.

The native bridge exposes only `start_brain_run`, `drain_brain_run`,
`submit_brain_host_result`, and `cancel_brain_run` for production runs. Rust
dispatches those calls to the canonical provider implementation.

## Ownership

### Rust Owns

- built-in brain ids, aliases, display names, and catalog revision;
- strategy ids, defaults, and effective-strategy diagnostics;
- module/provider compatibility and selection from profile/provider facts;
- provider-state mode, rebuild policy, fingerprints, and invalidation reasons;
- legal buffered-turn transitions and terminality;
- pending tool request identity and submitted-result correlation;
- tool failure counters, stop decisions, and output bounds;
- cancellation, timeout, bounded buffering, and cleanup;
- OpenAI Responses and pi-agent provider loops;
- roleplay narrator phase/FSM hosting;
- provider-state and transport result assembly;
- stable reason codes exposed to diagnostics and APIs.

### TypeScript Owns

- concrete MCP, web, browser, local-code, patch, memory, and service tool
  execution;
- profile prompt, soul, memory, and skill asset loading until their separate
  ownership migrations move them;
- service and adapter event projection, including Rusty View and Den
  Observation;
- bounded debug-cache persistence and browser-safe debug projection;
- external client mechanics that have no Rust implementation, but only behind
  an explicitly registered foreign-brain adapter;
- HTTP, SSE, CORS, and admin response envelopes.

TypeScript does not own brain identity, strategy selection, turn transitions,
retry/stop decisions, provider-state policy, or a built-in brain registry.

## Canonical Brain Catalog

The catalog lives in a focused Rust brain/runtime or config crate, not in
`core-engine/src/lib.rs` and not in the native bridge. Initial built-ins are:

| Canonical id | Provider protocol | Strategies | Execution |
| --- | --- | --- | --- |
| `pi-agent` | chat completions | `default`, `roleplay_narrator` | Rust |
| `openai-responses` | Responses | `replay`, `previous-response-chain` | Rust |

`pi-agent-core`, `rust-pi-agent`, and `local` are rejected module ids. Profile
and test fixtures use canonical `pi-agent`; no input canonicalization or alias
fallback remains.

The deterministic `local` brain is not a production catalog entry. Fake
provider clients and deterministic brains belong in Rust unit tests, package
test support, or smoke fixtures.

### Catalog DTOs

The Rust source emits or serializes reviewable DTOs equivalent to:

```text
BrainCatalog
  revision
  modules[]

BrainCatalogModule
  module_id
  display_name
  provider_protocols[]
  default_strategy_id
  strategies[]
  required_host_capabilities[]

BrainCatalogStrategy
  strategy_id
  provider_state_mode
  provider_state_rebuild
  diagnostics

BrainSelectionRequest
  configured_module_id?
  configured_strategy_id?
  provider_protocol
  provider_kind
  roleplay_narrator_enabled

BrainSelectionPlan
  catalog_revision
  module_id
  selected_strategy_id
  effective_strategy_id
  provider_state_policy
  strategy_diagnostics
  required_host_capabilities[]
```

TypeScript consumes generated or generated-checked projections. It does not
redeclare catalog contents.

## Host Capability Contract

The TypeScript boundary is a service-owned `BrainHostCapabilities` adapter. It
is not a brain module and cannot register built-in identities or strategies.

Initial capabilities are:

- `execute_tool`: resolve one Rust-selected neutral tool request against the
  profile's already validated tool profile and return a typed result;
- `project_debug_reference`: associate a call/provider request with a bounded
  debug record without affecting turn policy;
- `project_event`: forward a neutral event to service/browser/observation sinks
  without affecting coordination success.

The durable tool exchange is typed:

```text
HostToolExecutionRequest
  wake_id
  session_id
  call_id
  provider_item_id?
  tool_name
  arguments_json
  tool_profile_revision

HostToolExecutionResult
  call_id
  ok
  output_text
  retryable?
  action
  reason_code?
  summary?
  debug_detail_id?
```

`arguments_json` is the provider tool-call protocol payload. The model is not
asked to author orchestration, handoff, or lifecycle JSON. The host validates
arguments through the selected tool schema and reports typed failure facts.
Rust owns what those facts do to the turn.

Debug projection failure is non-blocking. Tool execution failure is returned to
the Rust coordinator as data. TypeScript must not convert either into hidden
turn terminality.

## Buffered Turn State Machine

The coordinator belongs in `crates/brains/brain-runtime` or a focused sibling
crate. Provider crates use it; the native bridge only converts calls and owns a
service-instance host handle.

States:

```text
created
  -> running
  -> awaiting_host_tools
  -> running
  -> completed

created | running | awaiting_host_tools
  -> failed | cancelled | timed_out
```

Rules:

- start is accepted exactly once per `(host, wake_id)`;
- stream sequence and terminal item are monotonic;
- pending tool call ids are unique within a wake;
- a result is accepted exactly once for a pending call id;
- duplicate identical submissions may return an idempotent receipt;
- conflicting duplicate submissions fail with a stable reason code;
- cancellation and timeout are terminal and clear pending host work;
- drain is an observation operation, not a state-authority operation;
- terminal runs remain drainable until their final queued items are observed,
  then are removed;
- item, request, output, and byte ceilings are explicit configuration;
- provider-specific metrics and credential updates are terminal attachments,
  not alternate lifecycle branches.

Task 5378 adds the tool failure and output-bound policy to this state machine.

## Bridge Operation Plan

The end-state native surface is provider-neutral:

- `brain_catalog`;
- `plan_brain_selection`;
- `start_brain_run`;
- `drain_brain_run`;
- `submit_brain_host_result`;
- `cancel_brain_run`;
- `buffered_brain_run_diagnostics`;
- `cleanup_buffered_brain_runs`.

Provider-specific start/drain/submit/cancel methods were deleted when OpenAI
Responses and pi-agent moved to the generic operations. They are not retained
as fallbacks.

The start request contains the selected catalog module/strategy, frozen wake
input, selected tool descriptors, provider configuration, and optional provider
state. Rust dispatches to the selected built-in implementation. TypeScript
registers a neutral `BrainHostExecutor` wake callback per profile; that callback
does not define brain identity, catalog entries, strategies, or lifecycle.

## Landed Export And Call-Site Disposition

| Former surface | Former consumer | Landed disposition |
| --- | --- | --- |
| `BrainModuleId`, profile `brain.module` parser | `profile-loading.ts` | Profile input remains a string; Rust `plan_brain_selection` validates it against the catalog. |
| `BrainModule`, `BrainModuleRegistry`, `createBrainModuleRegistry`, `defaultBrainModules` | runtime config and package surface | Deleted. Rust catalog is canonical. |
| `resolveBrainModuleSelection`, `resolveBrainModuleStrategy` | runtime registration/rebuild | Deleted; `plan_brain_selection` is authoritative. |
| strategy/provider-state metadata helpers | runtime diagnostics, rebuild, fingerprints | Rust selection plans return the metadata and own fingerprint policy. |
| `piAgentCoreBrainModule`, `rustPiAgentBrainModule` | runtime registration and smokes | Deleted; canonical `pi-agent` is required. |
| `openAiResponsesBrainModule` | runtime registration and smokes | Deleted; generic Rust catalog dispatch selects `openai-responses`. |
| `localBrainModule` | registry smoke/dev behavior | Deleted from production; deterministic executors exist only in smoke support. |
| provider client mode/config helpers | brain module and smokes | Production host adapters always request live clients; explicit fake clients are smoke support only. |
| Responses/pi-agent incremental drain loops | brain module | One generic host loop drains Rust coordinator directives. |
| tool request preparation/execution | brain module | Mechanics live in `tool-execution-host.ts`; policy lives in Rust. |
| debug-reference correlation | brain module | Debug projection is isolated and cannot affect coordinator policy. |
| roleplay narrator `createPhaseBrain` composition | brain module/narrator executor | Rust narrator receipts host phase sequencing; TS executes requested provider/tool work. |
| `BrainImplementation` and local brain helpers | `local-brain.ts`, package surface | Deleted/renamed. `brain-host-runtime.ts` exports the neutral `BrainHostExecutor`; deterministic brains live only in smoke support. |
| package `brain` exports | downstream packages/smokes | Host-capability and Rust catalog/readback exports remain; test fakes use smoke-support imports. |

Before cutover, production imports of `brain-module.ts` occurred in:

- `service-runtime-config.ts`;
- `profile-loading.ts`;
- `provider-state-fingerprints.ts`;
- `runtime-diagnostics.ts`;
- `package-surface/brain.ts`.

All production imports and exports are removed. Smokes use the generic host
contract or explicit smoke support.

## Completed Migration Sequence

1. Task 5374 implemented the coordinator transitions in Rust without changing
   provider behavior.
2. Task 5378 moved failure counters, stop policy, and output bounds to Rust.
3. Tasks 5382 and 5386 adapted Responses and pi-agent to generic coordinator
   operations and delete their old TypeScript drain policy.
4. Task 5414 moved catalog, selection, diagnostics, and provider-state
   fingerprint policy to Rust.
5. Task 5388 moved narrator phase hosting onto the Rust coordinator.
6. Task 5389 deleted `brain-module.ts`, production local brain exports,
   provider-specific bridge run operations, aliases, and fallback code.

Each step deletes the superseded path in the same task. There is no dual-write,
fallback registry, or old/new runtime toggle.

## Foreign Brain Adapters

Rusty Crew does not currently require a production foreign-language brain.
Do not preserve `BrainModule` for hypothetical extensibility.

If a real foreign brain is added later, it uses a separately named,
fail-closed `ForeignBrainAdapter` registration with:

- an explicit implementation id not colliding with built-ins;
- declared protocol and host capability requirements;
- bounded wake/stream/cancellation behavior;
- no ability to mutate the built-in catalog;
- the same neutral stream/action/provider-state validation.

This is an extension seam, not a legacy TS path.

## Validation Matrix

### Rust Deterministic

- catalog selection/default/invalid module/invalid strategy tests;
- provider protocol compatibility tests;
- every legal and illegal coordinator transition;
- duplicate and conflicting host result submissions;
- cancellation and timeout races;
- bounded stream/tool/output storage;
- provider-state rebuild/fingerprint behavior;
- narrator receipt JSON round trips, identity validation, phase/outcome
  matching, bounded review cycles, and restart behavior;

### Bridge And TypeScript

- manifest/native surface/mapping/schema/fingerprint gates;
- host tool execution success, denial, exception, and missing-tool smokes;
- debug projection failure remains non-blocking;
- runtime config registration/rebuild and `/model` diagnostics;
- no production import or export of `brain-module.ts`/`BrainModule`;
- no production deterministic/fake brain selection.

### Live Delivery

Use the SQLite debug service and Rusty View:

- OpenAI Responses text, reasoning, provider state, and a mid-turn tool;
- pi-agent chat-completions text, reasoning when supplied, and a mid-turn tool;
- one recoverable denied/unsuccessful tool result that the model can report;
- cancellation and idle-timeout behavior;
- roleplay narrator phase/tool/text/reasoning/final narrative flow;
- SSE reconnect/readback and `/model`/admin diagnostics.

Run the persistence-relevant registration/rebuild subset against the PostgreSQL
service without using live service data as disposable test state.

Task 5389's deterministic and live results are recorded in
`docs/rust-brain-catalog-live-certification-5389.md`.

## Consequences

- Production brain authority has one durable Rust home.
- TypeScript remains valuable for tools and adapters without becoming the
  default location for provider-loop policy.
- Adding a built-in brain requires a Rust catalog entry and Rust brain
  implementation, not a new TypeScript module object.
- Existing profile fixtures using transitional ids must be updated during the
  clean-break cutover.
- ADR 0021 remains historical context for why the neutral contract supports
  multiple implementation languages, but no longer defines the production
  ownership direction.
