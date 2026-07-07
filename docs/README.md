# `rusty-crew` docs

This directory holds implementation notes, ADRs, runbooks, audits, proof
slices, and historical records for Rusty Crew.

Rusty Crew is a Rust-owned coordination runtime with a TypeScript service host,
a TypeScript brain island built on `@earendil-works/pi-agent-core`, and
first-class Rust brain modules behind the same neutral
wake/stream/action/provider-state contract. Use this README as a local document
map; use Den project `rusty-crew`, especially the `rusty-crew-unified-architecture`
and `brain-body-architecture` documents, as the live architecture source of
truth.

When local companion docs conflict with the unified architecture, ADRs, current
code, or the repository root `README.md` / `AGENTS.md`, the unified architecture
wins. The `docs/historical/` subdirectory holds records that have been superseded
by landed work; see `docs/historical/README.md`.

## Source-of-truth posture

- **Den** owns current task state, durable planning notes, design docs, and
  user-facing coordination for project `rusty-crew`.
- **Repo docs** describe committed architecture, measurements, ADRs, and local
  implementation surfaces.
- **The code and tests** are the implementation truth when they conflict with
  old planning prose.
- **The unified architecture doc wins** when companion docs contradict it.

## Current source assumption

The TypeScript brain island uses the current `earendil-works/pi` source
(`https://github.com/earendil-works/pi`) and its published
`@earendil-works/pi-*` package names. The pin lives in
`pi-package-source-lock.md` and ADR `0001-current-pi-package-source.md` (in the
repo-root `adr/` directory). References in these docs to older local research
checkouts, older package locations, or version-skewed comparisons are
historical audit context only; they are not an implementation recommendation.

## Start here

0. **Den document `rusty-crew-unified-architecture`** — authoritative design.
   It supersedes recommendations in every local companion doc where they
   conflict. Activation/spawn/prompt mechanics live inside Rust; Crew-owned
   service storage is explicit; brain modules are first-class behind the
   neutral wake contract regardless of implementation language; Den is scoped to
   Den-owned planning/product data plus observability; and tool availability is
   profile-based rather than gated by a `WorkerPolicy` allow/deny model.

1. **`adr/` (both repo-root `adr/0001-*` and `docs/adr/0002`–`0022`)** — the
   decision trail. Notable recent decisions:
   - `0021-first-class-brain-modules.md` — brain modules are first-class behind
     the neutral wake contract; the Rust `openai-responses` brain is a peer of
     the TypeScript pi-agent brain, not an experiment.
   - `0022-crew-owned-service-storage.md` — Rusty Crew owns Crew service data
     (coordination, profiles, providers, transcripts, memory, lore, module
     data, telemetry, diagnostics), partitioned by durable concern rather than
     pushed into Den.
   - `0020-storage-backend-abstraction-and-postgresql-readiness.md` —
     backend-neutral storage facade; PostgreSQL is now a live backend, not a
     proof feature.
   - `0002-napi-brain-event-throughput.md` — JSON-string FFI over napi was
     measured, not assumed.
   - `0003-mid-turn-delta-policy.md` — frozen snapshots plus body-owned
     next-wake queuing with aggressive TTL.
   - `0006`/`0007`/`0008`/`0009`/`0010` — delegation runtime, optional worker
     pools, lifecycle guardrails, bounded fan-out, and Den observability.

   Companion implementation notes:
   - `service-composition-decomposition-plan.md` — current migration path for
     moving service composition out of the oversized brain-island service app
     without breaking the working service.
   - `rust-pi-agent-brain-port-contract.md` — parity matrix and cutover
     boundary for moving the current pi-agent brain from TypeScript to a Rust
     brain module without porting the unused pi-ai provider matrix.

2. **Two brain surfaces, one contract.** Rust coordination wakes a brain with a
   frozen `BodyState` snapshot; brain implementations emit
   `BrainWakeStreamItem`s, `BrainEvent`s, `BrainAction`s, and provider-state
   updates; Rust ingests the stream, validates accepted actions, owns
   lifecycle effects, and persists coordination state. Implementations today:
   - `crates/brains/openai-responses` — Rust brain for the OpenAI Responses
     API. Wired into production wake handling through the native bridge.
   - `ts/packages/brain-island` — TypeScript brain island built on
     `@earendil-works/pi-agent-core` / `pi-ai`. Owns the pi package
     integration, model-callable tool adaptation, profile/role assembly, and
     the roleplay narrator brain.

3. **Historical audit context (read-only, not binding).** The `pi-crew-*` and
   `pi-agent-rust-port-inspiration.md` / `rust-llm-ecosystem-research.md` docs
   record the parity and dependency research that motivated the rewrite. They
   are kept as evidence of *why* the boundary looks the way it does. They are
   not the current PRD: where they describe a TS-only LLM boundary, a
   `spawn_worker`/`prompt_worker` TS-called FFI verb, or a `WorkerPolicy`
   allow/deny tool gate, ADR 0021 and the unified architecture supersede them.

## Repository structure

```text
/rusty-crew
  /adr
    0001-current-pi-package-source.md
  /crates
    /core
      /core-protocol      # transport-free Rust protocol types
      /core-bus           # in-process coordination bus
      /core-session       # full/worker/delegated session registry
      /core-persistence   # Crew storage boundary: coordination + service data
      /core-body          # body projection, wake threshold, action executor
      /core-config        # runtime/profile config validation and normalization
      /core-tool-registry # tool metadata and registry ownership
      /core-engine        # composition crate for the Rust coordination service
    /bridge
      /core-bridge-api    # stable bridge-facing facade + manifest scaffold
      /core-bridge-node   # napi-rs native Node boundary
      /core-bridge-mock   # in-process test bridge
      /core-bridge-codegen # manifest/codegen placeholder
    /brains               # Rust brain modules behind the neutral wake contract
      /openai-responses
  /docs                   # architecture notes, ADRs, measurements, smokes
    /adr                  # 0002..0022
    /historical           # superseded records
    /profile-templates
  /governance
    ownership.toml        # crate/package dependency boundary rules
    storage-scope.toml    # which crate may own which storage schema/table prefix
  /tools                  # boundary + smoke runners (mjs)
  /ops/systemd            # local service unit files
  /ts
    /packages
      /contracts          # TypeScript contracts until codegen owns this
      /native-bridge      # native addon loader and TS bridge facade
      /brain-island       # pi Agent brain boundary and tool/profile assembly
      /service-host       # service process composition root
      /adapter-den        # Den data + observability adapter
      /adapter-mcp        # MCP client adapter
      /adapter-telegram   # Telegram channel adapter
      /adapter-tui        # local TUI adapter
```

The bridge was originally patterned on an external `runtime-bridge-api`
workspace. That lineage is historical context only; the current source map is
Rusty Crew-specific. The `governance/ownership.toml` per-crate `may_not_depend_on`
lists and `governance/storage-scope.toml` schema-prefix rules are the
machine-checkable boundary rules — extend them rather than relying on review
alone.

## Current implementation map

The repo lives at `/home/dev/rusty-crew` and
`https://github.com/FuzzySlipper/rusty-crew`. The local deployed service roots
are (see `local-service-topology.md` and `local-service-runbook.md`):

- `/home/system/rusty-crew` on port `9347` with PostgreSQL (schema
  `rusty_crew`) for long-lived agents;
- `/home/system/rusty-crew-debug` on port `9348` with SQLite for smoke tests,
  live certification, and disposable debug sessions.

Crate / package roles:

- `crates/core/core-protocol` — transport-free Rust protocol types.
- `crates/core/core-bus` — in-process event bus.
- `crates/core/core-session` — session registry.
- `crates/core/core-body` — body-loop wake threshold surface.
- `crates/core/core-config` — runtime/profile config validation boundary.
- `crates/core/core-tool-registry` — tool metadata and registry ownership.
- `crates/core/core-engine` — core composition crate.
- `crates/core/core-persistence` — backend-neutral Crew storage boundary for
  coordination and partitioned service data (SQLite default; PostgreSQL behind
  the `postgres` feature, now wired as a live backend).
- `crates/brains/openai-responses` — Rust brain module for the OpenAI
  Responses API.
- `crates/bridge/*` — bridge API, mock, native Node boundary, and codegen
  placeholder. `crates/bridge/core-bridge-api/bridge-manifest.toml` is the
  active unified manifest scaffold.
- `ts/packages/contracts` — generated-contract placeholder.
- `ts/packages/native-bridge` — TS bridge facade and native addon loader.
- `ts/packages/brain-island` — current pi package brain boundary, tool/profile
  assembly, and adapter-neutral service ports; also hosts the roleplay narrator
  brain.
- `ts/packages/service-host` — service process composition root, concrete
  adapter injection, startup entrypoint, host-level CORS, browser shell/static
  site mounting, and service-host smoke entrypoints. Some admin/API route
  wiring and background drain loops are still transitional inside
  `brain-island` behind explicit ports.
- `ts/packages/adapter-den`, `adapter-mcp`, `adapter-telegram`, `adapter-tui`
  — platform adapter boundaries.

First checks and CI gate:

```sh
npm ci
npm run verify:offline
```

`verify:offline` is the deterministic local gate and the command run by GitHub
Actions. It includes Rust fmt/clippy/tests, TypeScript typecheck, architecture
boundaries, runtime-config parity, bridge contract parity, bridge fixture
drift, and bridge validation. It must not require Den, live providers, local
Postgres, a running Rusty Crew service, or Rusty View. The architecture
boundary portion runs
`rust-crate-boundaries`, `ts-package-boundaries`, `storage-scope`, and
`production-fake-guard`; those checks live in `tools/`. See
`smoke-test-inventory.md` for the full smoke catalogue and execution lanes
(`offline`, `native-offline`, `local-service`, `debug-service`,
`local-infrastructure`, `live-provider`, and `rusty-view-certification`).
The TypeScript gate also runs `npm run smoke:validation-audit`, which keeps the
curated offline smoke subset from accidentally gaining Den/service/Postgres/UI
or live-provider requirements.

## LLM boundary decision

The Rust coordination core does **not** call LLM provider APIs as part of
coordination. Brain modules do call providers, and those modules may be
TypeScript or Rust. TypeScript owns the pi-agent brain integration and many
tool/provider surfaces; Rust brain modules are first-class only when they stay
behind the neutral wake/stream/action/provider-state contract and do not depend
on coordination internals, persistence, adapters, service-host code, or local
config. The lane is guarded by `governance/ownership.toml` and
`npm run smoke:rust-crate-boundaries`.

The old `prompt_worker` / `spawn_worker` operations are not TS-called FFI
verbs. Rust still owns activation, body projection, worker/delegation
lifecycle, action validation, and packet routing.

The current decision is recorded in `adr/0021-first-class-brain-modules.md`.
The pi package source decision is in `adr/0001-current-pi-package-source.md`
(repo-root `adr/`) and `pi-package-source-lock.md`.

## Decisions landed (formerly "open-question milestones")

The open questions from the original rewrite plan have been resolved through
the real path rather than detached mock spikes. The decision trail:

- Rust substrate routing, body projection, brain action validation, and
  session creation — landed in `core-engine` / `core-body` / `core-session`.
- TS brain island wired to current pi packages — `ts/packages/brain-island`.
- Native bridge around a real brain wake path —
  `crates/bridge/core-bridge-node`, measured in `ffi-throughput-napi.md`
  (baseline in `ffi-throughput-pre-napi.md`), decision in
  `adr/0002-napi-brain-event-throughput.md`.
- Mid-turn state deltas — frozen snapshots plus body-owned next-wake queuing
  with aggressive TTL; `adr/0003-mid-turn-delta-policy.md`.
- Full-agent to delegated-worker completion flow —
  `end-to-end-delegated-slice.md`; run locally with `npm run build:native` and
  `npm run smoke:delegated-slice`.
- Production wake scheduler — `adr/0004-wake-scheduler-ownership.md` and
  `production-wake-path-contract.md`.
- Tool availability — profile-based, not `WorkerPolicy`;
  `adr/0014-tool-profile-enforcement.md`.
- Rust brain modules as peers of the TS brain — `adr/0021-first-class-brain-modules.md`.
- Crew-owned service storage partitioned by concern —
  `adr/0022-crew-owned-service-storage.md`, with the scope map in
  `storage-scope-governance.md` and `storage-repository-split-map.md` enforced
  by `tools/check-storage-scope.mjs`.

The 2824 architecture closeout index lives in `2824-architecture-decision-index.md`;
the 2825 stub/fake audit lives in `2825-stub-fake-audit.md`; stub/fake/placeholder
policy lives in `stubs-fakes-placeholders-policy.md`, `adr/0015-test-seams-and-public-exports.md`,
and `adr/0016-runtime-clock-policy.md`. The parity-audit grounding snapshot is
`parity-open-questions-grounding.md` (a dated snapshot; re-check against code
before relying on its findings). The current maintenance/durability review
snapshot is `architecture-review-2026-07-06.md` (dated; code wins on conflict).

## What this service is *not*

- **Not a fork of pi-crew.** Rusty Crew is its own runtime with explicit
  migration/parity lessons from pi-crew and Hermes. Do not copy pi-crew
  worker-pool assumptions or manually mirrored Den tools as the default path.
- **Not a generic framework.** Rusty Crew owns a concrete service runtime:
  coordination, sessions, profiles, model providers, transcripts, memory,
  lore, local tools, adapters, and diagnostics. It should stay modular without
  becoming a pile of optional abstractions.
- **Not Den storage.** Den owns Den planning/product/observability data.
  Rusty Crew owns Crew service data and partitions it by durable concern inside
  Crew storage.
- **Not TS-only LLM execution.** TypeScript brains remain first-class, but Rust
  brain modules are also supported behind the same neutral contract.
- **Not a performance project.** The rewrite's primary value is
  *enforcement* of the architectural boundary. Performance is incidental and
  should not drive design decisions; if Rust happens to be faster at the
  worker state machine, that's a side effect, not a goal.
