# rusty-crew

Rusty Crew is the Rust-owned coordination runtime for agent services that grew
out of pi-crew. The near-term goal is a local service that can host multiple
full/prime agents, delegate bounded subagent work, connect to Den services, and
keep deterministic coordination state in Rust while TypeScript owns LLM calls,
tools, profiles, skills, MCP, and platform adapters.

This repository is no longer a bare scaffold. It has a working Rust engine,
native bridge, TypeScript brain island, service host, profile loading, tool
registry, admin diagnostics/control surfaces, Den successor adapters, and local
field-test configuration under `/home/agents/rusty-crew`.

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
  fan-out accounting, completion routing, counters, and SQLite persistence.
- `crates/bridge/core-bridge-api` is the stable bridge-facing Rust facade.
  `crates/bridge/core-bridge-node` exposes the native Node transport.
- `ts/packages/contracts` mirrors bridge-visible TypeScript contracts.
- `ts/packages/native-bridge` loads the native bridge and maps Rust wire shapes
  into TypeScript.
- `ts/packages/brain-island` owns profile loading, role assembly, pi-agent
  integration, model calls, model-callable tools, service runtime config,
  admin/debug APIs, and production brain wake wiring.
- `ts/packages/adapter-den` owns Den successor Gateway integration,
  observation/conversation/delivery/timeline projections, and Den memory
  client helpers.
- `ts/packages/adapter-mcp`, `adapter-telegram`, and `adapter-tui` are adapter
  boundaries for MCP, Telegram, and operator TUI/debug surfaces.

## Service Layout

The local service is expected to use:

- config: `/home/agents/rusty-crew/config`
- data: `/home/agents/rusty-crew/data`
- logs: `/home/agents/rusty-crew/logs`
- run state: `/home/agents/rusty-crew/run`

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

Use focused smokes while developing, then broaden before handoff:

```sh
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
npm install
npm run typecheck
npm run format
```

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
- TypeScript owns LLM providers, pi-agent integration, profile composition,
  model-callable tool definitions/execution, MCP clients, skills, memory
  clients, and platform adapters.
- Den is product data plus observability. Den services are not the internal
  coordination bus.
- Platform adapters should be isolated so Den Channels, Telegram, MCP, and
  future connectors can change without reshaping Rust coordination state.
- Queues must be treated cautiously. Durable or body-owned queues require
  explicit TTL and should never resurrect expired instructions or messages.
- Intentional stubs/fakes need an attached follow-up task so temporary behavior
  does not disappear into the codebase.
