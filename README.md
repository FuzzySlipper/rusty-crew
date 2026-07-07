# rusty-crew

Rusty Crew is the Rust-owned coordination runtime for agent services that grew
out of pi-crew. The near-term goal is a local service that can host multiple
full/prime agents, delegate bounded subagent work, connect to Den services, and
keep deterministic coordination state in Rust while brain modules run behind a
neutral wake contract. TypeScript owns the pi-agent brain integration, many
tools, profiles, skills, MCP clients, and platform adapters; Rust brain modules
are also supported when they stay behind the same wake/stream/action contract.

This repository is no longer a bare scaffold. It has a working Rust engine,
native bridge, TypeScript brain island, service host, profile loading, tool
registry, admin diagnostics/control surfaces, Den successor adapters, and local
field-test configuration under `/home/system/rusty-crew`.

## Source Of Truth

- The architecture principles live in Den project `rusty-crew`, especially
  `rusty-crew-unified-architecture` and `brain-body-architecture`.
- Local docs in `docs/` are implementation notes and ADRs. When an older parity
  audit conflicts with current code or the unified architecture, treat the audit
  as feature inventory rather than binding design.
- The current README is intended as the quick operational map for agents.

## Current Shape

- `crates/core/core-protocol` defines transport-free protocol types,
  `BrainAction`, sessions, tool profiles, channel records, MCP records, and
  coordination event shapes.
- `crates/core/core-engine` owns deterministic coordination: sessions,
  messages, body projection, brain action validation, delegation lifecycle,
  fan-out accounting, completion routing, counters, and backend-neutral
  persistence.
- `crates/bridge/core-bridge-api` is the stable bridge-facing Rust facade.
  `crates/bridge/core-bridge-node` exposes the native Node transport.
- `ts/packages/contracts` mirrors bridge-visible TypeScript contracts.
- `ts/packages/native-bridge` loads the native bridge and maps Rust wire shapes
  into TypeScript.
- `ts/packages/brain-island` owns profile loading, role assembly, pi-agent
  integration, model-callable tools, brain wake wiring, and adapter-neutral
  ports consumed by the service.
- `ts/packages/service-host` is the service composition root. It owns service
  startup scripts, concrete adapter injection, the HTTP listener, host-level
  CORS, browser shell/static-site mounting, and service-host smoke entrypoints.
  Some admin/API route wiring and background drain loops are still transitional
  inside `brain-island` behind explicit ports while decomposition continues.
- `crates/brains/*` contains direct Rust brain modules that implement provider
  loops behind the neutral wake/stream/action/provider-state contract. These
  crates may not reach into Rust coordination internals.
- `ts/packages/adapter-den` owns Den successor Gateway integration,
  observation/conversation/delivery/timeline projections, and Den memory
  client helpers.
- `ts/packages/adapter-mcp`, `adapter-telegram`, and `adapter-tui` are adapter
  boundaries for MCP, Telegram, and operator TUI/debug surfaces.

## Service Layout

The local machine intentionally runs two Rusty Crew services:

- live agent service: `/home/system/rusty-crew`, port `9347`, PostgreSQL;
- debug/test service: `/home/system/rusty-crew-debug`, port `9348`, SQLite.

Use the debug service for smoke tests, live certification, frontend debugging,
and disposable LLM experiments. Use the live service for long-lived agents and
project/channel activity that should not be polluted by test profiles.

The service host should bind admin/debug HTTP surfaces on `0.0.0.0` in this
trusted LAN development environment. Tokens and service URLs belong in local
config/env files, not in repo docs.

Useful commands:

```sh
npm run service:start
npm run service:debug-turn -- --help
npm run smoke:service-host
npm run smoke:admin-diagnostics-api
npm run smoke:admin-control-api
```

## Tools And Delegation

Tool availability is profile-based. Profiles request toolsets or concrete tool
names; the canonical registry in `ts/packages/brain-island/src/tool-registry.ts`
produces the selected `ToolProfile` that Rust records on the session. Do not
reintroduce pi-crew's older `WorkerPolicy` allow/deny model as the primary tool
gate.

The production brain resolver currently includes local code tools, web/browser
tools, Den memory tools, dense profile memory, skills tools, planning tools,
curator execution, channel readback, and delegation tools. Delegation tools are
model-callable helpers that enqueue `BrainAction::RequestDelegation` actions;
Rust still owns child session creation, wake scheduling, lineage, fan-out
policy, completion routing, timeout, cancellation, and cleanup.

Delegation toolset:

- `spawn_subagent`
- `fan_out_subagents`
- `scout_codebase`
- `summarize_files`
- `find_relevant_paths`

Proof commands:

```sh
npm run smoke:delegation-tools
npm run smoke:delegated-slice
npm run smoke:delegated-role-assembly
npm run smoke:delegated-resource-cleanup
npm run smoke:production-delegation-wake
```

## Build And Test

Pinned local toolchains:

- Rust `1.96.0` with `rustfmt` and `clippy` from `rust-toolchain.toml`;
- Node `v26.2.0` and npm `11.16.x` from `.nvmrc` / `package.json` engines.

Native bridge builds require the usual local compiler toolchain plus Node/npm:
`cargo`, `rustc`, `npm`, and `napi-rs` build dependencies installed by
`npm ci`. PostgreSQL and SQLite CLIs are needed only for backup/live-storage
operations, not for the offline CI gate.

Use focused smokes while developing, then run the offline gate before handoff.
GitHub Actions runs the same offline gate on pushes to `main`, pull requests,
and manual dispatch:

```sh
npm ci
npm run verify:offline
```

`verify:offline` is intentionally free of Den, live providers, local Postgres,
running Rusty Crew services, and Rusty View. It expands to:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run typecheck
npm run test:unit
npm run format
npm run smoke:architecture-boundaries
npm run smoke:runtime-config-parity
npm run smoke:external-cassettes
npm run smoke:bridge-contract-parity
npm run smoke:bridge-native-surface
npm run smoke:bridge-fixture-drift
npm run smoke:bridge-fingerprint-drift
npm run smoke:bridge-validation
```

Native bridge runtime artifacts are build output. Fresh checkouts build them
with `npm run build:native`; the repo commits
`ts/packages/native-bridge/native/index.d.ts` as the declaration surface but
does not commit generated `.node` or loader `.js` files. The policy and guard
are documented in `docs/native-bridge-artifact-strategy.md` and checked by
`npm run smoke:native-artifact-tracking`.

TypeScript unit tests use Node's built-in `node:test` runner through `tsx`.
Add package-local tests under `ts/packages/<package>/test/*.test.ts` for pure
logic that should not require a native build, service startup, Den, Rusty View,
or live providers. Use smokes for broader integration paths, and use Rusty
View live certification for substantial chat/runtime deliverables where the
user-visible path matters. The Crew-side live evidence rule and completion
template are documented in `docs/live-deliverable-certification.md`.

External response-shape cassettes live under `fixtures/external-cassettes/`.
They are committed only after headers, tokens, cookies, provider secrets, and
overly-large prompt/provider payloads are removed or normalized. Offline
cassette smokes, such as `npm run smoke:external-cassettes`, preserve shape
evidence from Den/provider/UI integrations without making CI depend on live
systems.

Use the smoke runner to inspect and run integration proofs without adding more
one-off root aliases:

```sh
npm run smoke -- --list
npm run smoke -- --list --package brain-island
npm run smoke -- brain
```

Smoke categories, environment-requirement flags, and the rule for moving new
smokes out of package `src/` are documented in
`docs/smoke-test-inventory.md`. Use `npm run smoke -- --list --lane offline`
or another lane filter when deciding whether a check is CI-safe, local-service
only, live-provider backed, or a Rusty View certification run.

Common focused checks:

```sh
npm run smoke:tool-registry
npm run smoke:tool-registry-parity
npm run smoke:tool-profile-selection
npm run smoke:tool-session-selection
npm run smoke:local-code-tools
npm run smoke:memory-skills-wake
npm run smoke:planning-runtime-wake
npm run smoke:mcp-surfaces-e2e
npm run smoke:den-successor-service
```

## Pi Packages

Use the current `https://github.com/earendil-works/pi` source and the published
`@earendil-works/pi-*` packages for the TypeScript brain island. Older local
checkout references in docs are historical audit context only. The current
package pin is tracked in `docs/pi-package-source-lock.md`.

## Architecture Rules

- Rust owns deterministic coordination, persistence, lifecycle validation,
  action acceptance/rejection, body projection, wake thresholds, delegation,
  completion routing, and runtime counters.
- Brain modules may be TypeScript or Rust. TypeScript owns the pi-agent brain
  integration and many tool/provider/adapter surfaces; Rust brain modules are
  allowed only behind the neutral wake/stream/action/provider-state contract.
- Rusty Crew owns Crew service data: coordination state, profiles, provider
  state, transcripts, memory, lore, module data, telemetry, and diagnostics.
- Den owns Den product/planning/observability data. Den services are not the
  internal coordination bus and are not the storage fallback for Crew service
  data.
- Platform adapters should be isolated so Den Channels, Telegram, MCP, and
  future connectors can change without reshaping Rust coordination state.
- Queues must be treated cautiously. Durable or body-owned queues require
  explicit TTL and should never resurrect expired instructions or messages.
- Intentional stubs/fakes need an attached follow-up task so temporary behavior
  does not disappear into the codebase.
