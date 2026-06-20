# Rusty Crew Local Bootstrap

Project-specific live guidance and planning live in Den project `rusty-crew`.
Historical planning still exists in `pi-crew`; prefer `rusty-crew` for new
project docs, task updates, and guidance.

Primary Den documents:

- `[doc: rusty-crew/rusty-crew-unified-architecture]`
- `[doc: rusty-crew/brain-body-architecture]`

## Source-Of-Truth Posture

This local file is bootstrap context for agents entering the repository. It is
not the current planning queue.

- **Den** owns current task state, durable planning notes, design docs, and
  user-facing coordination.
- **Repo docs** describe committed architecture, measurements, ADRs, and local
  implementation surfaces.
- **The code and tests** are the implementation truth when they conflict with
  old planning prose.
- **The unified architecture doc wins** when companion docs contradict it.

## Architecture Soul

> Rust owns deterministic coordination. TypeScript owns brain capability and
> external adapters. The bridge manifest defines the border.

- Rust is authoritative for coordination: internal bus routing, sessions,
  body-state projection, wake thresholds, brain action validation, delegated
  worker lifecycle, completion packet persistence, and restart hydration.
- TypeScript owns the brain island, the current
  `@earendil-works/pi-agent-core` / `@earendil-works/pi-ai` integration, tool
  execution, role/profile composition, and platform adapters.
- Den owns product data and observability. Den is not the internal
  agent-to-agent coordination bus.
- The brain receives frozen state snapshots and emits structured actions. It
  must not reach around Rust coordination internals.
- Worker spawning and prompting are Rust-owned lifecycle operations, not
  TypeScript-called FFI verbs.
- Tool availability is profile-based. Do not reintroduce `WorkerPolicy` as the
  main tool gate.

## Repository Structure

```text
/rusty-crew
  /adr
    0001-current-pi-package-source.md
  /crates
    /core
      /core-protocol      # transport-free Rust protocol types
      /core-bus           # in-process coordination bus
      /core-session       # full/worker/delegated session registry
      /core-persistence   # SQLite coordination store and hydration
      /core-body          # body projection, wake threshold, action executor
      /core-engine        # composition crate for the Rust coordination service
    /bridge
      /core-bridge-api    # stable bridge-facing facade + manifest scaffold
      /core-bridge-node   # napi-rs native Node boundary
      /core-bridge-mock   # in-process test bridge
      /core-bridge-codegen # manifest/codegen placeholder
  /docs                   # architecture notes, ADRs, measurements, smokes
  /governance
    ownership.toml        # crate/package ownership boundary rules
  /ts
    /packages
      /contracts          # TypeScript contracts until codegen owns this
      /core-bridge        # TS bridge facade
      /native-bridge      # native addon loader
      /brain-island       # pi Agent/LLM boundary
      /adapter-den        # Den data + observability adapter
      /adapter-*          # remaining adapter boundaries
```

## Local Commands

```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace -- -D warnings
cargo test --workspace

# TypeScript and docs formatting
npm run format
npm run typecheck

# Native bridge
npm run build:native
npm run smoke:bridge-wake

# Focused smokes
npm run smoke:den
npm run smoke:brain
npm run smoke:mid-turn
npm run smoke:delegated-slice

# Throughput measurements
npm run measure:napi
cargo run --release -p rusty-crew-core-bridge-node --bin measure_brain_event_throughput
```

`smoke:delegated-slice` expects local den-router to be reachable, defaulting to
`http://127.0.0.1:18082`. It should not require an external API key.

## Agent Lane Quick Reference

| Lane             | Language | Crate/Package Dir                                            | May Not                                                                         |
| ---------------- | -------- | ------------------------------------------------------------ | ------------------------------------------------------------------------------- |
| rust-protocol    | Rust     | `crates/core/core-protocol`, `crates/bridge/core-bridge-api` | Depend on native bridge or TS runtime details                                   |
| rust-bus         | Rust     | `crates/core/core-bus`                                       | Know about Den, Node, or platform adapters                                      |
| rust-session     | Rust     | `crates/core/core-session`                                   | Own product data or adapter behavior                                            |
| rust-body        | Rust     | `crates/core/core-body`                                      | Call LLMs or platform APIs directly                                             |
| rust-persistence | Rust     | `crates/core/core-persistence`                               | Mirror Den product records into coordination storage                            |
| rust-engine      | Rust     | `crates/core/core-engine`                                    | Move brain or adapter implementation details into core                          |
| bridge-native    | Rust/TS  | `crates/bridge/*`, `ts/packages/*bridge*`                    | Expand manifest surface for convenience-only behavior                           |
| ts-brain         | TS       | `ts/packages/brain-island`                                   | Route coordination around Rust                                                  |
| ts-adapter       | TS       | `ts/packages/adapter-*`                                      | Make coordination decisions or block internal routing on observability failures |
| contract-steward | Rust/TS  | `crates/bridge/core-bridge-api`, `ts/packages/contracts`     | Hand-edit generated artifacts once codegen owns them                            |

## Design Principles

- **Architecture before feature surface:** prove the coordination substrate
  before growing agent-facing conveniences.
- **In-process coordination:** agent-to-agent routing stays in Rust, not Den or
  platform adapters.
- **Explicit degraded observability:** adapter projection failures should be
  visible but must not stop internal coordination.
- **Frozen snapshots first:** mid-turn events queue for the next wake unless a
  later design explicitly proves safe interruption.
- **Boring bridge contracts:** keep napi and manifest operations small,
  explicit, and measured.

## TypeScript House Style

TypeScript in this repo is boundary code and brain/adaptor code. Prefer longer,
clearer code over compact cleverness. Name conversions across Rust/TS shapes
explicitly, especially snake_case/camelCase and branded contract types. Avoid
ambient registries or hidden runtime coupling unless they are established by the
pi packages being wrapped.

When dealing with pi packages, use the current packages from
`https://github.com/earendil-works/pi`. Older local checkout references are
historical audit context only.

## Rust House Style

Rust in this repo should be boring coordination authority code. Prefer explicit
state, explicit errors, explicit events, and narrow crate APIs. Keep adapter,
LLM, and Node concerns out of core crates. If persistence helpers accept dynamic
names, whitelist them.

`cargo clippy --workspace -- -D warnings` is a required check — it must pass
with zero warnings before any Rust change is considered done. Run it alongside
`cargo fmt --all --check`. Clippy is not part of `cargo build` or `cargo test`;
it is a separate command that must be invoked explicitly. Treat all clippy
warnings as errors to fix, not lint suggestions to suppress with `#[allow]`
unless there is a documented reason in a code comment.
