# rusty-crew

Rusty-Crew is the Rust coordination-core rewrite for pi-crew. The current
scaffold is intentionally small: it establishes crate/package boundaries and
keeps the implementation pointed at the unified architecture before the deeper
bridge, persistence, and brain-island work begins.

The authoritative design lives in Den project `pi-crew`, document
`rusty-crew-unified-architecture`. Local docs in `docs/` are supporting
material. When they conflict, the unified architecture wins.

## Current Shape

- `crates/core/core-protocol` — transport-free protocol types shared by the
  Rust core and bridge API.
- `crates/core/core-bus` — in-process coordination bus for agent-to-agent
  routing and event fanout.
- `crates/core/core-session` — session registry for full, worker, and delegated
  sessions.
- `crates/core/core-persistence` — SQLite-backed local coordination store for
  sessions, message history, worker-run records, completion packets, and
  restart hydration. Den product data is intentionally excluded.
- `crates/core/core-body` — body-loop wake threshold and deterministic state
  projection surface.
- `crates/core/core-engine` — composition crate for the Rust coordination
  service.
- `crates/bridge/core-bridge-api` — stable bridge-facing facade with no native
  transport dependency. Its `bridge-manifest.toml` is the active unified
  manifest scaffold. It also owns the shared runtime-buffer lease protocol used
  by bridge transports.
- `crates/bridge/core-bridge-node` — native Node transport boundary. napi-rs
  glue belongs here; the current slice exposes the manifest surface and tests
  runtime-buffer ownership without leaking native dependencies into core crates.
- `crates/bridge/core-bridge-mock` — in-process bridge for early integration
  tests.
- `crates/bridge/core-bridge-codegen` — manifest/codegen placeholder.
- `ts/packages/contracts` — TypeScript contract placeholder until codegen owns
  this package.
- `ts/packages/core-bridge` and `ts/packages/native-bridge` — TS bridge facade
  and native loader placeholder.
- `ts/packages/brain-island` — TS brain island boundary. This is where the
  current `@earendil-works/pi-*` Agent/LLM dependency belongs.
- `ts/packages/adapter-den` — first platform adapter boundary. It injects Den
  product-data updates into the Rust bus and projects internal coordination
  events back to Den as best-effort observability.
- `ts/packages/adapter-*` — remaining platform adapter package placeholders.

## Build And Test

```sh
cargo fmt --all --check
cargo test --workspace
npm install
npm run typecheck
npm run format
npm run smoke:den
```

## Architecture Rules

- Rust owns deterministic coordination: bus routing, sessions, body state, wake
  thresholds, action validation, packet lifecycle, and coordination
  persistence.
- Rust persistence stores coordination state under `engine_data_dir`; Den
  task/project/document records remain in Den and are not mirrored into the
  SQLite store.
- TypeScript owns the brain island, tool execution, LLM provider calls, profile
  composition, and platform adapters.
- Den is product data and observability. It is not the coordination bus.
  Projection failures are explicit degraded/dropped observability state; they
  must not block internal agent-to-agent routing.
- Worker spawning and prompting are internal Rust lifecycle/activation
  operations, not TS-called FFI verbs.
- Large bridge wake payloads cross as `RuntimeBufferHandle`s. See
  `docs/runtime-buffer-ownership.md` for acquire/release rules.
- Tool availability is profile-based. Do not reintroduce a `WorkerPolicy`
  allow/deny gate as the main tool model.
- Use the current `https://github.com/earendil-works/pi` source for the pi
  packages. Older local checkout references in docs are historical audit
  context only. The current package pin is tracked in
  `docs/pi-package-source-lock.md`.

## Integrated Milestones

The project should build toward the architecture open questions in place:

1. Reconcile docs, scaffold, and manifest with the unified architecture.
2. Define a unified manifest/codegen source of truth.
3. Build the real Rust coordination substrate: bus, sessions, body state, and
   action validation.
4. Wire the TS brain island to current pi packages.
5. Add the Den adapter as data/observability projection, not coordination
   authority.
6. Add Rust-owned coordination persistence.
7. Implement the native bridge and settle `RuntimeBufferHandle` ownership in the
   real wake path.
8. Measure FFI event throughput through that integrated path.
9. Resolve mid-turn delta behavior against the real upstream Agent hooks.
10. Prove a planner-to-worker delegated slice end to end.

Avoid mock-only spikes for the open questions unless the mock is directly
exercising the same path that will become production.
