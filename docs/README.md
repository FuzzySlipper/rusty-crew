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

1. **`pi-crew-upstream-audit.md`** — what's actually used from
   `@earendil-works/pi-agent-core` and `@earendil-works/pi-ai` today.
   5 runtime symbols, 51 type-only imports, ~3 actual call sites. Read
   this first to understand the floor.

2. **`pi-crew-extraction-analysis.md`** — could we just *vendor* the
   upstream code we use? Yes, but the cost is ~18 KLOC of which 16 KLOC
   is one auto-generated file (`models.generated.ts`). This document
   derives the tier table and explains why the rewrite is the cheaper
   path.

3. **`pi-crew-core-bridge-manifest.md`** — the FFI contract for the
   rewrite. 12 operations, 6 handle types, 1 typed error channel, ~25
   protocol types. **This is the PRD for the implementation agent.**
   It includes the LLM boundary decision (TS-side LLM, Rust-side
   authority) and the anti-patterns the manifest is designed to
   prevent.

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

## LLM boundary decision (the big one)

The Rust core does **not** call any LLM provider API. `streamSimple`,
`getModels`, `getProviders`, and the upstream `Model<Api>` shape stay
in the TypeScript front-end. The Rust core's `WorkerHandle` is the
*supervised worker* — it has a `prompt_worker` operation and an event
stream, but the LLM call is a TS-side concern.

The trade is documented in `pi-crew-core-bridge-manifest.md` §"LLM
boundary". The alternative (vendor the LLM surface into Rust) is
~18 KLOC + 25 external SDKs; the chosen path keeps the LLM surface in
TS where it already has working code and lets the Rust core focus on
authority, lifecycle, and the packet protocol.

This decision is recorded in `adr/0001-current-pi-package-source.md`.

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
  Den stack. Those stay TS. The Rust core is *only* the worker pool,
  packet protocol, session/persistence lifecycle, and policy enforcement.
- **Not a performance project.** The rewrite's primary value is
  *enforcement* of the architectural boundary. Performance is
  incidental and should not drive design decisions; if Rust happens to
  be faster at the worker state machine, that's a side effect, not a
  goal.
