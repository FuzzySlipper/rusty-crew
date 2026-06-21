# `rusty-crew` — pi-crew core rewrite (Rust)

This directory holds the design documents for a Rust rewrite of the
pi-crew worker-pool core. The existing TypeScript implementation lives at
`/home/dev/pi-crew` and is the source of truth for *what the system
does*; the goal of the rewrite is to express the same behavior with a
hard architectural boundary that prevents the kinds of drift we already
see (the orchestrator replacing the worker's stream at construction,
the supervisor re-mapping the same event twice, the completion packet
config struct with 12 optional fields, etc.).

## Current source assumption

When the rewrite needs the upstream pi packages for the TypeScript brain
island, use the current `earendil-works/pi` source
(`https://github.com/earendil-works/pi`) and its published
`@earendil-works/pi-*` package names. References in these docs to older local
research checkouts, older package locations, or version-skewed comparisons are
historical audit context only; they are not an implementation recommendation.

## Start here

0. **Den document `rusty-crew-unified-architecture`** — authoritative design.
   It supersedes recommendations in every local companion doc where they
   conflict. In particular, it moves activation/spawn/prompt mechanics inside
   Rust, scopes Den to product data plus observability, and replaces
   `WorkerPolicy`-style tool gates with profile-based tool enablement.

1. **`pi-crew-upstream-audit.md`** — what's actually used from
   `@earendil-works/pi-agent-core` and `@earendil-works/pi-ai` today.
   5 runtime symbols, 51 type-only imports, ~3 actual call sites. Read
   this first to understand the floor.

2. **`pi-crew-extraction-analysis.md`** — could we just *vendor* the
   upstream code we use? Yes, but the cost is ~18 KLOC of which 16 KLOC
   is one auto-generated file (`models.generated.ts`). This document
   derives the tier table and explains why the rewrite is the cheaper
   path.

3. **`pi-crew-core-bridge-manifest.md`** — historical draft manifest and
   anti-pattern catalog. It remains useful for typed handles, error-channel
   discipline, codegen shape, and protocol inventory, but it is no longer the
   literal PRD for the FFI surface. Do not implement its obsolete
   `spawn_worker` / `prompt_worker` TS-called FFI verbs or `WorkerPolicy`
   allow/deny model; use the unified architecture instead.

4. **`pi-agent-rust-port-inspiration.md`** — someone has done a Rust
   port of the upstream pi packages at `/home/research/pi_agent_rust`.
   It's a single mega-crate (~322 KLOC) and tracks a *newer* upstream
   than the older audit snapshot, so it's **not** a candidate for vendoring or
   forking. It is also not the source for the TS brain island; use
   `https://github.com/earendil-works/pi` for that. But it contains several
   focused files that are excellent
   reference implementations for specific pieces of the manifest — most
   notably `src/agent_cx.rs` (capability-scoped context wrapper),
   the `AgentEvent` enum in `src/agent.rs` (event vocabulary with
   serde `tag = "type"` discipline), and the `AbortHandle`/`AbortSignal`
   pair. The doc also flags `src/error.rs` as a *cautionary example*
   (flat `String`-typed variants — exactly the anti-pattern the manifest
   warns against). Read this before starting implementation; it tells
   you which pi_agent_rust files are worth opening as a reference and
   which to leave alone.

5. **`rust-llm-ecosystem-research.md`** — scout of the Rust LLM/agent
   crate landscape (2026-06). Covers Rig (the standout multi-provider
   framework, 7K+ stars), `genai` (broader provider coverage, 27+
   providers), Claw Code's 6-crate Rust workspace (the closest prior
   art to what pi-crew's manifest wants to build), `schemars` for
   JSON-Schema-from-Rust-types, and the MCP/structured-output crates
   for future scope. The key recommendation: **don't add LLM-side Rust
   dependencies for v1** (LLM is TS-side per the manifest), but borrow
   the *shapes* — Rig's `Tool` trait with associated types, Claw
   Code's typed `ApiError` enum — when designing `ToolExecutorDescriptor`
   and `CoreBridgeError`. Read before choosing any dependency.

6. **`pi-package-source-lock.md`** — current source/version pin for the
   `@earendil-works/pi-*` packages used by the TypeScript brain island.

## Structural model

Patterned on `/home/dev/asha/engine-rs/crates/bridge/runtime-bridge-api/`.
The asha `bridge-manifest.toml` + `bridge-emit.py` codegen pipeline
is the reference implementation; pi-crew's manifest mirrors the shape
but covers a different domain (multi-agent worker pool, not a
simulation engine).

| Layer | asha analog | pi-crew analog |
|-------|-------------|----------------|
| FFI manifest | `runtime-bridge-api/bridge-manifest.toml` | `pi-crew-core-bridge-manifest.md` |
| Boundary types (no transport deps) | `runtime-bridge-api/src/lib.rs` | `engine-rs/crates/bridge/core-bridge-api/src/` |
| Native transport | `bridge/native-bridge/` (napi-rs) | `engine-rs/crates/bridge/core-bridge-node/` |
| Mock transport | (test crate) | `engine-rs/crates/bridge/core-bridge-mock/` |
| Codegen | `harness/codegen/bridge-emit.py` | `engine-rs/crates/bridge/core-bridge-codegen/` |
| TS facade | `ts/packages/runtime-bridge` | `ts/packages/core-bridge` |
| Generated contracts | `ts/packages/contracts` | `ts/packages/contracts` |
| TS loaders | `ts/packages/native-bridge` | `ts/packages/native-bridge` |

The ownership/depgraph rules from asha's `governance/ownership.toml`
should be ported over (with the per-crate `may_not_depend_on` lists)
so the rewrite inherits the same machine-checkable dependency rules.

## Current scaffold

The initial repo scaffold now exists in `/home/dev/rusty-crew` and
`https://github.com/FuzzySlipper/rusty-crew`.

- `crates/core/core-protocol` — transport-free Rust protocol types.
- `crates/core/core-bus` — in-process event bus.
- `crates/core/core-session` — session registry.
- `crates/core/core-body` — body-loop wake threshold surface.
- `crates/core/core-engine` — core composition crate.
- `crates/bridge/*` — bridge API, mock, native Node placeholder, and codegen
  placeholder. `crates/bridge/core-bridge-api/bridge-manifest.toml` is the
  active unified manifest scaffold.
- `ts/packages/contracts` — generated-contract placeholder.
- `ts/packages/core-bridge` and `ts/packages/native-bridge` — TS bridge facade
  and loader placeholder.
- `ts/packages/brain-island` — current pi package brain boundary.
- `ts/packages/adapter-*` — platform adapter placeholders.

First checks:

```sh
cargo fmt --all --check
cargo test --workspace
npm install
npm run typecheck
npm run format
```

## LLM boundary decision

The Rust core does **not** call any LLM provider API. `streamSimple`,
`getModels`, `getProviders`, and the upstream `Model<Api>` shape stay
in the TypeScript brain island/front-end. In the unified architecture the Rust
core drives activation and wakes the TS brain with a frozen state snapshot; the
old `prompt_worker` operation is not a TS-called FFI verb.

The trade is documented in `pi-crew-core-bridge-manifest.md` §"LLM
boundary". The alternative (vendor the LLM surface into Rust) is
~18 KLOC + 25 external SDKs; the chosen path keeps the LLM surface in
TS where it already has working code and lets the Rust core focus on
authority, lifecycle, and the packet protocol.

This decision is recorded in `adr/0001-current-pi-package-source.md`.

## Integrated open-question milestones

The open questions should be answered through the real path as it comes online,
not through detached mock spikes:

1. Build enough Rust substrate to route events, project body state, validate
   brain actions, and create sessions.
2. Wire the TS brain island to the current pi packages.
3. Implement the native bridge around a real brain wake path.
4. Settle `RuntimeBufferHandle` ownership using large state/prompt payloads in
   that path.
5. Measure FFI event throughput using the real bridge/body/brain stream.
6. Resolve mid-turn state deltas by testing actual upstream Agent hook behavior.
7. Prove a minimal full-agent to delegated-worker completion flow.

The true napi throughput measurement for item 5 is recorded in
`ffi-throughput-napi.md`, with the resulting hybrid batching decision in
`adr/0002-napi-brain-event-throughput.md`.

The mid-turn delta decision for item 6 is recorded in
`adr/0003-mid-turn-delta-policy.md`: v1 uses frozen snapshots plus
body-owned next-wake queuing with aggressive TTL rather than pi-agent internal
queues as durable state.

The current parity-audit grounding for task 2986 is recorded in
`parity-open-questions-grounding.md`. The production wake scheduler decision
for task 2988 is recorded in `adr/0004-wake-scheduler-ownership.md`, and the
implementation contract for task 2830 is recorded in
`production-wake-path-contract.md`.

The remaining 2824 architecture open questions are recorded in:
`2824-architecture-decision-index.md`,
`adr/0012-single-engine-service-scope.md`,
`adr/0013-wake-buffer-assembly-ownership.md`,
`adr/0014-tool-profile-enforcement.md`, and
`adr/0011-steer-followup-frozen-snapshot.md`.

Stub, fake, placeholder, failure-injection, and clock policy decisions are
recorded in `stubs-fakes-placeholders-policy.md`,
`adr/0015-test-seams-and-public-exports.md`, and
`adr/0016-runtime-clock-policy.md`.

The current 2825 stub/fake reconciliation audit is recorded in
`2825-stub-fake-audit.md`.

Bridge helper classification for task 2839 is recorded in
`adr/0005-bridge-surface-and-diagnostics.md`: the manifest is the stable
protocol spec for v1, while hand-written bindings may expose explicitly
classified setup, runtime-local, or diagnostic helpers until codegen matures.

The prime-agent delegation runtime decision for task 2840 is recorded in
`adr/0006-prime-agent-delegation-runtime.md`: direct subagent delegation comes
before worker-pool leasing, with worker pools preserved as a later capacity
layer rather than the default architecture.

The production delegation request shape for task 2842 is documented in
`delegation-request-contract.md`.

The delegated-worker slice for item 7 is documented in
`end-to-end-delegated-slice.md`; run it locally with `npm run build:native` and
`npm run smoke:delegated-slice`.

## What this rewrite is *not*

- **Not a fork of pi-crew.** The TS implementation keeps shipping at
  `/home/dev/pi-crew` and stays the production system. The Rust core
  is a parallel implementation that, when mature, replaces the
  `pi-service` runtime. The TS front-end (`pi-crew`, `pi-channels`,
  `pi-mcp`, `pi-tools`, `pi-profiles`, `pi-core`, `pi-memory`,
  `pi-governance`) is largely unchanged.
- **Not a complete rewrite.** The TS-side packages above are the
  front-end. They are the things that talk to Den Channels, render the
  TUI, drive the cron subsystem, and integrate with the rest of the
  Den stack. Those stay TS. The Rust core is *only* the coordination substrate:
  bus, activation, body loop, packet protocol, session/persistence lifecycle,
  and action validation.
- **Not a performance project.** The rewrite's primary value is
  *enforcement* of the architectural boundary. Performance is
  incidental and should not drive design decisions; if Rust happens to
  be faster at the worker state machine, that's a side effect, not a
  goal.
