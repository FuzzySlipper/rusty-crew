# pi_agent_rust port — file-by-file inspiration for the pi-crew core rewrite

> **Author note for the implementing agent:** Someone has already done a Rust port of upstream's `pi-agent-core` + `pi-ai` at `/home/research/pi_agent_rust`. **We are NOT using it as a dependency or forking it.** It is a single mega-crate (~322 KLOC in `src/`) and bakes in extensions/RPC/swarm/provider-matrix machinery pi-crew does not want. For the TypeScript brain island, use the current `earendil-works/pi` source (`https://github.com/earendil-works/pi`) and the `@earendil-works/pi-*` package names; older local checkouts are audit context only.
>
> **But** several focused files inside it are excellent reference implementations for specific pieces of the manifest. This document lists the ones worth reading (and what to lift vs. leave behind), mapped to the operations in `pi-crew-core-bridge-manifest.md`.
>
> **Pre-read:** `pi-crew-core-bridge-manifest.md` (the PRD), `pi-crew-upstream-audit.md` (what we use from upstream today).

## Why not vendor / fork

| Concern                                  | pi_agent_rust reality                                         | Implication for the rewrite                                            |
|------------------------------------------|---------------------------------------------------------------|------------------------------------------------------------------------|
| Crate shape                               | One `pi` crate (`src/lib.rs`, ~322 KLOC) + `pi` bin           | Doesn't give us the layered `core-bridge-api` / `core-bridge-node` / `core-bridge-mock` / `core-bridge-codegen` split the manifest is patterned on (per `README.md` "Structural model") |
| Architecture                              | Tightly coupled — `agent.rs` is 12,225 lines and uses ~40 internal modules | Pulling anything out requires dragging in 100s of KLOC of unrelated surface |
| Upstream version                          | Tracks a version-skewed Rust port rather than the current `https://github.com/earendil-works/pi` TypeScript source used by the brain island | API shape has drifted — `AgentEvent` gained `AutoCompactionStart`, `AutoRetryStart`, `ExtensionError` variants pi-crew doesn't want |
| Surface bloat                             | Extensions (`extensions.rs`, 2 MB), WASM (`pi_wasm.rs`), RPC (`rpc.rs`, 320 KB), TUI (`interactive.rs`, 127 KB), provider matrix (`providers/*.rs`), swarm/ledger (`swarm_*.rs`) | ~90% of the LOC is unrelated to the FFI surface we want |
| Public API policy                         | The `sdk.rs` module **explicitly** documents the model we want (line 7-9: *"Use [`sdk`] as the stable library-facing surface"*), but the `sdk` module is a thin re-export facade over the same internal types — it doesn't enforce that callers don't reach into internals | Useful as a *pattern reference* (re-export curated subset, mark rest `#[doc(hidden)]`), not as a foundation |

**Verdict:** treat pi_agent_rust as a **reading library**, not a code source. Use the specific files listed below as reference implementations; do not `cargo add` this crate or copy non-trivial logic from it without translating to the manifest's architecture.

## Caveats when reading pi_agent_rust

1. **Upstream drift.** Treat local research checkouts and quoted audit paths as historical snapshots. When comparing TypeScript brain shapes for implementation, compare against the current `https://github.com/earendil-works/pi` source and the `@earendil-works/pi-*` package names.
2. **Monolithic crate.** Everything is in one `pi` crate. Treat each file as an *example of how to express one concept in Rust*, not as a candidate crate to lift. The rewrite wants small crates with strict `may_not_depend_on` rules (per the README's reference to asha's `governance/ownership.toml`).
3. **Cargo features matter.** Look at `Cargo.toml` `[features]` for `tui`, `sqlite-sessions`, `wasm-host`, `jemalloc`, `fuzzing` — files guarded by these features (e.g. `tui.rs`, `session_sqlite.rs`, `pi_wasm.rs`) are NOT relevant to the core rewrite.

---

## Files worth reading — direct value to the rewrite

The mapping below goes: **file → manifest operation it informs → what to lift → what to ignore**.

### Tier 1 — Direct reference implementations for manifest operations

#### `src/agent_cx.rs` (263 lines) — capability-scoped context wrapper

> The cleanest single file in the entire repo. This is exactly the *kind* of focused wrapper the manifest wants for `WorkerHandle` / `SessionHandle`.

Maps to: **WorkerHandle**, **SessionHandle**, **EngineHandle** in `pi-crew-core-bridge-manifest.md` §"Handle taxonomy".

What to lift:
- The `AgentCx::for_testing() / for_testing_with_io() / for_request() / for_request_with_budget()` factory pattern (lines 42-78). Deterministic-context construction is *exactly* the gap the rewrite needs — see `pi-crew-core-bridge-manifest.md` §"initialize_engine" (`clock: "system" | { fixed: "..." }` field).
- The capability accessor pattern: `cx.fs()` / `cx.time()` / `cx.http()` / `cx.process()` returning typed `AgentFs<'_>` / `AgentTime<'_>` / `AgentHttp<'_>` / `AgentProcess<'_>` zero-sized-or-newtype handles (lines 88-108). This is the *Rust idiom* for the manifest's "TS side cannot forge a handle" rule — capabilities are borrowed from a context, not constructed standalone.
- The `Deref<Target = Cx>` impl (line 111-117). Lets you pass `&AgentCx` anywhere an underlying context is expected without wrapping boilerplate. Consider doing the same for the manifest's handles where it makes sense.

What to ignore:
- The actual `asupersync::Cx` dependency. The rewrite should not adopt `asupersync`; use `tokio` or whatever the project standard is. The *shape* of the wrapper is the value, not the underlying runtime.

#### `src/agent.rs` lines 935-1043 — `AgentEvent` enum definition

> This is the most directly useful thing in the whole port. The enum variant set maps almost 1-to-1 onto the existing TS `AgentEvent` union (see `legacy_pi_mono_code/pi-mono/packages/agent/src/types.ts:179-194`) which is what pi-crew's `GatewayEvent` vocabulary (44 kinds) is already a superset of.

Maps to: **`subscribe_events`** in `pi-crew-core-bridge-manifest.md` (event vocabulary must be unchanged from `pi-core/src/events.ts`).

What to lift:
- The `#[serde(tag = "type", rename_all = "snake_case")]` derive on the enum (line 933-934). This is exactly the wire-format discipline the FFI needs — the TS side already speaks `event.type` as a discriminator. **Mirror this on every protocol type the codegen produces** so the TS facade can `switch (event.type)` directly.
- The variant set shape:
  - `AgentStart { session_id }` / `AgentEnd { session_id, messages, error }` — agent lifecycle
  - `TurnStart { session_id, turn_index, timestamp }` / `TurnEnd { session_id, turn_index, message, tool_results, latency_breakdown }` — turn lifecycle
  - `MessageStart { message }` / `MessageUpdate { message, assistant_message_event }` / `MessageEnd { message }` — message lifecycle
  - `ToolExecutionStart { tool_call_id, tool_name, args }` / `ToolExecutionUpdate { tool_call_id, tool_name, args, partial_result }` / `ToolExecutionEnd { tool_call_id, tool_name, result, is_error }` — tool lifecycle
  All variants use snake_case Rust field names with `#[serde(rename = "camelCase")]` (e.g. line 938 `session_id → sessionId`). The rewrite should pick **one** convention and stick to it; the audit doc warns the existing TS uses camelCase while Rust convention is snake_case. The pi_agent_rust pattern (snake_case Rust + `#[serde(rename = "camelCase")]` on individual fields) is the right answer because it keeps idiomatic Rust while preserving the TS wire shape.

What to ignore:
- `AutoCompactionStart` / `AutoCompactionEnd` / `AutoRetryStart` / `AutoRetryEnd` / `ExtensionError` (lines 1007-1042) — pi-crew does not have a compaction subsystem or extension layer; if those concerns get added later they go in separate event kinds, not into `AgentEvent`.

#### `src/agent.rs` lines 1051-1110 — `AbortHandle` / `AbortSignal`

> Clean, idiomatic Rust implementation of the abort-channel pattern. The manifest's `abort_worker` operation needs exactly this shape.

Maps to: **`abort_worker`** and the `reason` field in `pi-crew-core-bridge-manifest.md`.

What to lift:
- The `Arc<AbortSignalInner>` shared between `AbortHandle` and `AbortSignal` (lines 1061-1065) — the standard pattern for a multi-producer / single-abortor notification.
- The `AtomicBool` + `Notify` pair in `AbortSignalInner` (line 1062-1065) — lock-free flag, `tokio::sync::Notify` for waiter wake-up. The `is_aborted()` check on line 1095 is correct under the `SeqCst` ordering because it's the only writer.
- The `wait()` loop on line 1098-1109 — note the *two-condition* pattern: `is_aborted()` is checked before AND after `notified()`, because spurious wake-ups and races between concurrent abort calls can leave you notified without `aborted == true`.
- The factory pattern `AbortHandle::new() -> (Self, AbortSignal)` (line 1070-1081) — returns the handle-and-signal pair in one call so the caller can't accidentally forget the signal.

What to ignore:
- `tokio::sync::Notify` if the project standardizes on something else; the pattern is the same.

#### `src/provider.rs` lines 1-100 — `Provider` trait + `Context` struct

> This is what the `Provider` trait looks like in idiomatic Rust. The manifest does not actually define a Provider trait (LLM is TS-side), but **the structural discipline here is worth copying** for any TS-implemented-via-Rust-boundary the rewrite does need.

Maps to: design precedent for the FFI boundary in general; specifically the `LLM boundary` decision in `pi-crew-core-bridge-manifest.md` (§"LLM boundary").

What to lift:
- The `#[async_trait]` + `Send + Sync` supertrait on `Provider` (line 28-29). Forces every backend to be safely shareable across threads — a constraint the FFI manifest should impose on every type that crosses the boundary.
- The `Context<'a>` struct with `Cow<'_, str>` and `Cow<'a, [Message]>` (line 61-72). The borrow-vs-own split is the right way to express "I might own this, I might borrow from a longer-lived buffer" — exactly the use case for `RuntimeBufferHandle` in the manifest. Consider whether `WorkerRoleAssembly`, `CompletionPacket`'s message batch, etc. should be `Cow<'_, T>` over a `RuntimeBufferHandle`.
- The `Context::owned(...)` constructor (line 86-98) for the escape hatch — when the lifetime can't be threaded through, you can convert to fully-owned. The manifest's `RuntimeBufferHandle` reference-counting is the equivalent.

What to ignore:
- The actual provider implementations in `providers/*.rs` — pi-crew is keeping LLM on the TS side per the LLM boundary decision.

#### `src/sse.rs` lines 1-100 (whole file 1806 lines) — `SseParser`

> The rewrite manifest does not put SSE parsing in scope (it's TS-side LLM streaming), but **if the team ever changes its mind and brings any streaming into Rust** (e.g. for a tool side-channel), this is a reference-quality implementation.

What to lift (for future reference, not for direct copying now):
- The state machine: `buffer: String`, `current: SseEvent`, `has_data: bool`, `bom_checked: bool`, `scanned_len: usize` (lines 39-49). All fields justified, no premature optimization.
- The `MAX_EVENT_DATA_BYTES = 100 * 1024 * 1024` cap (line 11) — every parser of untrusted streams needs a per-event size cap. **The manifest doesn't have a similar cap for `RuntimeBufferHandle` content; worth adding.**
- The `intern_event_type()` static-string matching (lines 79-100) — replaces per-event `String` allocation with `Cow::Borrowed` for known event names. Eliminates one allocation per event on the LLM streaming hot path. The equivalent for the FFI event channel is **string interning on the 44 `GatewayEventKind` values** — they are a closed set, no allocation should ever happen for them.

What to ignore:
- The whole Anthropic/OpenAI event-name intern list (lines 86-100) — provider-specific.

### Tier 2 — Architectural patterns, not direct code

#### `src/sdk.rs` lines 1-100 + re-exports — the **stable API facade pattern**

> This is the single most important *pattern* in the whole port for the rewrite. Even though the rewrite is structured differently (multiple crates per the asha model), the *philosophy* is exactly right.

What to lift:
- The two-line module docstring at the top: *"Prefer importing from `pi::sdk` instead of deep internal modules."* + the `compile_fail` doctest example (lines 17-21) that demonstrates internal types are intentionally not accessible. **The rewrite's `ts/packages/contracts` should have the same discipline** — every TS import of a protocol type should come through `core-bridge-codegen`-generated code, not from a hand-typed mirror in the TS source. The `compile_fail` example is the right way to enforce it via the type system.
- The `pub use crate::agent::{ ... };` re-export block (lines 31-35). Whitelist, not denylist. The manifest's `pi-crew-core-bridge-manifest.md` §"Protocol types" should generate exactly this kind of `pub use` from the manifest, with everything else `#[doc(hidden)]` per the Rust convention (already established in `lib.rs` line 56-67 — every internal module is `#[doc(hidden)] pub mod`).
- The `BUILTIN_TOOL_NAMES: &[&str]` constant (lines 51-58) — a single closed-list constant of tool names, used both for `pub fn create_*_tool()` factories (lines 83-127) and for runtime registration. The rewrite's `register_tool` operation needs the same: a closed-set of `ToolExecutorDescriptor::kind` variants with named constants per kind.

What to ignore:
- The actual tool factories (lines 78-127) — they're tool implementations, not types.

#### `src/error.rs` lines 1-64 — `Error` enum — **CAUTIONARY EXAMPLE**

> This is what the rewrite manifest explicitly warns *against*. Read it to understand the failure mode.

Maps to: **`CoreBridgeError` / `CoreBridgeErrorKind`** in `pi-crew-core-bridge-manifest.md` §"Error channel".

What to lift — **the wrong things**, to make sure you don't replicate them:
- The `pub enum Error { Config(String), Session(String), Provider { provider, message }, Tool { tool, message }, ... }` pattern (line 12-64) is a flat `String`-typed variant set with no machine-readable discriminator. The manifest's `CoreBridgeErrorKind` is a closed enum precisely to avoid this — `Error::Provider { provider, message }` here forces every caller to string-match on `message` to know whether the error is retryable.
- `Config(String)`, `Session(String)`, `Auth(String)`, `Validation(String)`, `Extension(String)`, `Api(String)` — six variants that all carry a free-form `String` and nothing else. **The orchestrator cannot dispatch on these.** The rewrite manifest's `CoreBridgeErrorKind` is a fixed enum with a *separate* human-readable `message: String` field, which is the correct shape.
- The `#[error("...")]` `thiserror` derives throughout — fine for `Display`, but the derive is also the only thing distinguishing variants. The orchestrator needs `Debug` + a typed enum, not a `Display` string.

What to lift:
- The **idea** of having an `AuthDiagnosticCode` typed enum (lines 67-82) as a *nested* discriminator on the auth errors. The rewrite's `CoreBridgeErrorKind` should grow typed subcodes the same way when needed (e.g. `PolicyDenied` might want a `PolicyDeniedKind` enum for "denied because: tool-not-allowed / host-not-allowed / workdir-out-of-scope / max-duration-exceeded" without a giant top-level enum).
- The `is_retryable_error(...)` helper at line 995 — even though the underlying `Error` type is untyped, there's a separate function classifying retryability. This is a smell; the rewrite should encode retryability *in the type* (a `Retryable` impl on `CoreBridgeErrorKind`), not in a side-channel function.

#### `src/agent.rs` lines 620-699 — `AgentConfig` + `ToolApprovalHandler` + `ToolApprovalDecision`

> The closest analog in pi_agent_rust to the manifest's `WorkerPolicy` + `WorkerRoleAssembly` typed inputs. Worth reading for the `Arc<dyn Fn(...) -> BoxFuture>` callback pattern.

Maps to: **`spawn_worker`** input envelope (`WorkerSpawnRequest { role_assembly, model, policy, binding, ... }`).

What to lift:
- The `Arc<dyn Fn(...) -> BoxFuture<'static, ...> + Send + Sync>` callback type (line 685-686). This is the right type for any FFI-visible policy hook: `Send + Sync` (so it crosses threads), `BoxFuture<'static>` (so it's not bound to the caller's stack frame), explicit type signature. The manifest's `WorkerPolicy` shape needs the same for any "hook" fields — don't accept raw closures across the FFI (impossible), but if the core itself grows policy hooks (e.g. pre-tool-call `PolicyDenied` decisions), use this type.
- The `ToolApprovalDecision::Allow | Deny { reason: String }` enum (line 670-674) — typed allow/deny with structured reason. The manifest's `PolicyDenied` error variant carries a `String reason`; consider whether a typed reason enum is worth it (probably yes if there are >3 distinct denial reasons).
- The `AgentConfig` `Default` impl using a `resolved_max_tool_iterations_default()` helper (line 688-699) — defaults should not be inline literals in the struct definition. Pull them into named consts/fns so they're auditable.

What to ignore:
- `SemanticContextBundleInjection` (lines 702-749) — pi-crew does not have a semantic-context subsystem.

### Tier 3 — Read for context, no direct value

These files are mentioned only so the implementing agent doesn't waste time digging into them.

| File | Why you'd open it | Why to put it down again |
|------|-------------------|--------------------------|
| `src/lib.rs` | Sets the tone: `#[doc(hidden)]` on every internal module (line 56-67). **This is the model for `core-bridge-api/src/lib.rs`.** | The `forbid(unsafe_code)` and clippy allowances are project-specific style choices. |
| `src/agent.rs` (full 12k lines) | Where the `Agent` struct lives. | You do NOT want to copy any of it. The implementation is monolithic, ties together provider selection, message queueing, tool execution, extension dispatch — concerns the rewrite explicitly separates. |
| `src/models.rs` (185k lines) | Model registry + provider auth lookup. | This is the actual port of `models.generated.ts`. If the team ever wants to bring `getModels`/`getProviders` into Rust (the manifest's Tier 2 alternative), this is where the data lives. Otherwise: irrelevant. |
| `src/session.rs` (12k lines) | The session persistence implementation. | Way too big; pi-crew has its own `session_materialized_*` machinery. Read for *shape only* (records, states, snapshots). |
| `src/session_sqlite.rs` (967 lines) | The SQLite-backed session store. | Closer to the manifest's `PersistenceFailure` scope, but it uses `sqlmodel_core` and `asupersync`. Read for the migrations pattern. |
| `src/sdk.rs` lines 100-300 (tool factories) | How to register tools as `Box<dyn Tool>`. | pi-crew has its own tool model (`AgentTool`, `guarded-tool-types.ts`). The Rust `Box<dyn Tool>` trait-object pattern is informative but not directly applicable. |
| `src/providers/*.rs` | LLM provider implementations (Anthropic, OpenAI, etc.) | Per the LLM boundary decision, **all of this stays TS-side**. Do not import. |
| `src/conformance_shapes.rs`, `src/extension_*.rs`, `src/rpc.rs`, `src/swarm_*.rs`, `src/interactive*` | Extension system, RPC, swarm ledger, TUI. | None of these subsystems exist in pi-crew. Do not be tempted to lift them. |

---

## Concrete mapping: manifest operation → pi_agent_rust inspiration

| Manifest operation | Reference file(s) in pi_agent_rust | Notes |
|--------------------|-----------------------------------|-------|
| `initialize_engine` | `src/agent_cx.rs` (whole) | `for_testing() / for_request_with_budget() / for_current_or_request()` factory pattern. The `clock: "system" | { fixed: "..." }` field should be encoded the same way — a typed enum, not a `String`. |
| `shutdown_engine` | None directly | pi_agent_rust doesn't model drain-timeout; derive from the manifest's existing `agent-supervisor.ts` semantics in pi-crew. |
| `spawn_worker` | `src/agent.rs` lines 620-700 (`AgentConfig` + `ToolApprovalHandler`) | The `Arc<dyn Fn(...) -> BoxFuture<'static, ...> + Send + Sync>` pattern for any policy hooks. |
| `prompt_worker` | `src/agent.rs` lines 1051-1110 (`AbortHandle` / `AbortSignal`) | The handle/signal pair pattern. The manifest already has `WorkerHandle`; `AbortHandle` is the inner abort channel. |
| `abort_worker` | Same as above | The `reason` field in the manifest is the `Arc<AbortSignalInner>` plus the typed reason variant. |
| `await_worker_completion` | None directly | pi_agent_rust has no analog of `WorkerFinalState`. The manifest's enum (`Completed / Failed / Blocked / Exhausted / Aborted / Timeout`) is the right shape — preserve it. |
| `register_tool` / `revoke_tool` | `src/sdk.rs` lines 51-58 (`BUILTIN_TOOL_NAMES`) + `src/agent.rs` lines 685-686 (`ToolApprovalHandler`) | The closed-set `ToolExecutorDescriptor::kind` variants should be a single enum constant; the policy hook callback type is the right shape for any "approve tool call" hook the core exposes. |
| `get_worker_state` | None directly | The manifest's `WorkerRuntimeState` is the right shape. |
| `create_session` / `archive_session` / `get_session_state` | `src/sdk.rs` lines 406-444 (`AgentSessionHandle`, `AgentSessionState`, `SessionPromptResult`, `SessionTransportEvent`, `SessionTransportState`) | Useful for the *enum shapes* but the rewrite wants its own vocabulary mapped to `CompletionPacket` from `pi-core/src/types.ts`. |
| `subscribe_events` / `unsubscribe_events` | `src/sdk.rs` lines 162-275 (`SubscriptionId`, `EventListeners`, `subscribe`, `unsubscribe`, `notify`) | The `SubscriptionId(u64)` newtype + `EventListeners` registry is the right shape for the manifest's `SubscriptionHandle`. **Also see:** `src/agent.rs` lines 935-1043 (`AgentEvent` enum + `#[serde(tag = "type", rename_all = "snake_case")]`) — the event discriminator that the TS side `switch`es on. |
| `register_model_catalog` | `src/models.rs` (whole) | Only relevant if the team reverses the LLM boundary decision and brings `getModels`/`getProviders` into Rust (manifest's Tier 2 alternative). If so, this is the data file. Otherwise: not in scope. |
| `resolve_model` | `src/model_selector.rs`, `src/models.rs` (line 499 `model_autocomplete_candidates`) | Same scope caveat as above. |

---

## Anti-patterns to actively avoid (also from pi_agent_rust)

For each of these, pi_agent_rust does the wrong thing; the rewrite should not.

1. **Single mega-crate.** `src/lib.rs` re-exports 90+ modules; everything depends on everything. The rewrite's `core-bridge-api` / `core-bridge-node` / `core-bridge-mock` / `core-bridge-codegen` split with `governance/ownership.toml` `may_not_depend_on` rules (per the README "Structural model" section) is **the** architectural commitment the rewrite exists to make. Do not let `cargo` convenience collapse it.
2. **Untyped error variants.** `src/error.rs` lines 12-64 — `pub enum Error { Config(String), Session(String), ... }`. The manifest's `CoreBridgeErrorKind` is the right shape: closed enum + separate `message: String`. See `pi-crew-core-bridge-manifest.md` §"Error channel".
3. **`is_retryable_error` as a side-channel function.** `src/error.rs` line 995. Retryability should be encoded in the type (`CoreBridgeErrorKind` variant implies retryability), not a separate classifier function that callers might forget to call.
4. **`Arc<Arc<Arc<T>>>` over-cloning.** The Rust code uses `Arc<AssistantMessage>`, `Arc<ToolResultMessage>` (in `src/model.rs`) for cheap cloning during streaming. Fine in pi_agent_rust's monolithic context, but the rewrite's FFI marshalling layer should use `RuntimeBufferHandle` (reference-counted + serialized across the boundary) instead of cloning across threads — the manifest's "buffer ownership" open question #3 (line 491) needs to settle this.
5. **Compaction / extension / retry events in the core event enum.** `src/agent.rs` lines 1007-1042 — `AutoCompactionStart/End`, `AutoRetryStart/End`, `ExtensionError` are mixed into `AgentEvent`. The manifest wants `GatewayEvent` to be the 44 kinds from `pi-core/src/events.ts`, unchanged. If those concerns get added later, they are separate event kinds or separate event channels.

---

## Summary — the one-page takeaway

| | Use it | Ignore it |
|---|---|---|
| **Architecture** | The `sdk.rs` *facade pattern* (whitelist re-exports, `#[doc(hidden)]` internals). Apply to `core-bridge-api/src/lib.rs`. | The single-crate `pi` lib. Use the asha layered-crate model. |
| **Capability context** | The `AgentCx` wrapper + capability accessors (`cx.fs() / cx.time() / cx.http() / cx.process()`). Pattern after it for `EngineHandle`. | The `asupersync::Cx` dependency. |
| **Event vocabulary** | The `#[serde(tag = "type", rename_all = "snake_case")]` + `#[serde(rename = "camelCase")]` field-level discipline. Apply to every protocol type. | The `AutoCompaction*` / `AutoRetry*` / `ExtensionError` variants. |
| **Abort** | The `Arc<AbortSignalInner> { AtomicBool + Notify }` pattern. Use for `WorkerHandle`'s abort channel. | The `tokio::sync::Notify` if the project standardizes elsewhere. |
| **SSE** | The `MAX_EVENT_DATA_BYTES` cap + `intern_event_type` Cow pattern. Add a similar per-buffer cap to `RuntimeBufferHandle`. | The Anthropic/OpenAI provider specifics. |
| **Tool registration** | The closed-set `BUILTIN_TOOL_NAMES` const + `pub fn create_*_tool()` factory pattern. | The `Box<dyn Tool>` implementations. |
| **LLM providers** | Nothing (TS-side per the boundary decision). | All of `providers/*.rs`. |
| **Errors** | The *idea* of a typed diagnostic subcode enum (cf. `AuthDiagnosticCode`). | The flat `String`-typed `Error` enum. |

**One-line summary for the implementing agent:** `pi_agent_rust` is worth reading like a cookbook — `agent_cx.rs` and the `AgentEvent` enum + `AbortHandle` pair are recipes you should adapt; the rest of the crate is ingredients you don't want on the menu.
