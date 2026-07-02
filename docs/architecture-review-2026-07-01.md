# Architecture Review (2026-07-01)

Read-only examination of the repository as of commit `de2a31a`. Companion
implementation plan: `architecture-remediation-plan.md`.

## Overall assessment

The macro-architecture is well-conceived and well-documented: a deterministic
Rust coordination core, a TypeScript brain island for LLM/tool work, a
manifest-defined native bridge as the border, machine-checkable dependency
guardrails (`governance/ownership.toml`), and a real ADR trail with
measurements. The crate/package dependency graph is clean in both languages
and the ownership rules hold (protocol depends on nothing, brains depend only
on protocol + bridge-api, adapters depend only on contracts).

The problems are almost all one level down: the boundaries were drawn once at
the crate/package level and never applied *within* them, several core
principles have quietly drifted, and a few load-bearing mechanisms (contract
mirroring, the synchronous bridge, smoke-only testing) will not scale with
where the code is heading. Findings below in priority order.

## 1. Contract triplication across the bridge, with codegen still a stub

The bridge manifest defines **97 operations**. Each exists in three
hand-maintained forms:

- the Rust napi export surface (`crates/bridge/core-bridge-node/src/lib.rs`,
  ~5,900 lines, 130 `#[napi]` items),
- the hand-mirrored TypeScript contracts
  (`ts/packages/contracts/src/index.ts`, 184 exports),
- a ~5,000-line hand-written mapping layer
  (`ts/packages/native-bridge/src/index.ts`) full of
  `JSON.parse(...) as unknown` casts.

Meanwhile `core-bridge-codegen/src/main.rs` is 8 lines that count manifest
entries. Cross-boundary type safety is *asserted*, not checked — a renamed
Rust field produces `undefined` in TS at runtime, not a compile error. At 97
operations this is the single highest-leverage improvement: generate the TS
contracts and the snake_case/camelCase mapping from the manifest (or from the
Rust types via schema derive), or at minimum add runtime schema validation at
the boundary. The AGENTS.md `contract-steward` lane anticipates exactly this;
it never happened, and the surface grew ~100 operations wide in the meantime.

## 2. A Rust-brain LLM call blocks the entire Node event loop

The napi bridge contains **zero async functions**.
`run_openai_responses_brain_json` (`core-bridge-node/src/lib.rs:2490`) is a
synchronous call that internally drives `reqwest::blocking`
(`crates/brains/openai-responses/src/lib.rs:7`). It is wrapped in a cosmetic
`async` at `native-bridge/src/index.ts:2993` and invoked from production wake
wiring at `brain-island/src/brain-module.ts:435`. There is no `worker_threads`
usage anywhere in the TS packages.

Since everything — Telegram adapter, MCP, admin HTTP, TUI, all the
`setInterval` drain loops — runs in one Node process, the whole service stalls
for the full duration of a streamed LLM response (idle timeout alone is 30s;
real turns can be minutes). The returned "stream" is also fully materialized
into an array before crossing the bridge, so there is no incremental delivery
either.

## 3. The coordination store has become the product database

The governance rule says rust-persistence "may not mirror Den product records
into coordination storage," and the Architecture Soul says "Den owns product
data." Yet `core-persistence` now owns roleplay lore layers, conversation
branches/variants/snapshots, attachments, data-bank scopes, profile memory,
and a simple-KV store — visible in the import block of `postgres_proof.rs`
and in 58 inline `CREATE TABLE` statements. That is an application database,
not coordination state.

This scope creep is the root cause of the size problem:
`core-persistence/src/lib.rs` is ~29,000 lines (~21,800 production) with a
single `CoordinationStore` exposing ~290 public functions, and it is why the
bridge ballooned to 97 operations (many are CRUD verbs for product data —
exactly the "convenience-only surface expansion" the bridge lane forbids).
Either the principle should be formally amended (an ADR saying Rust now owns
durable product/memory data — ADR 0018/0019 gesture this way) or the
product-shaped storage should move behind a separate crate/store. Right now
docs and code disagree, and `ownership.toml` cannot catch it because it only
checks crate dependencies, not schema scope.

## 4. Monolithic files defeat the modular workspace

The workspace *looks* modular, but the real code lives in a handful of giant
single files:

| File                                    | Lines  |
| --------------------------------------- | ------ |
| core-persistence/src/lib.rs             | 28,897 |
| core-persistence/src/postgres_proof.rs  | 18,640 |
| brain-island/src/service-host.ts        | 11,997 |
| core-engine/src/lib.rs                  | 5,981  |
| core-bridge-node/src/lib.rs             | 5,882  |
| native-bridge/src/index.ts              | 5,043  |

Notably, `core-persistence/src/repositories.rs` already defines the right
decomposition — but it is only a *descriptive catalog* of repository groups
("while the monolithic implementation is split into repository modules," per
its own doc comment); the split never happened. The seams are named; use
them: one module per repository group in persistence, one module per route
family in service-host.ts, one module per operation family in the bridge.

`core-bus`/`core-session`/`core-body` are healthy thin crates, but
core-engine (130 public functions) has absorbed all domain logic — sessions,
delegation, fan-out, scheduler, memory spaces — and needs internal modules at
minimum.

## 5. Testing is 109 bespoke smoke scripts with no test harness

There is no test runner in the TS workspace — no vitest/jest/node:test, zero
`*.test.ts` files. Instead there are **114 `smoke-*.ts` files living inside
`brain-island/src`** (shipped inside the package source tree) wired to 109 of
the 119 npm scripts in a ~12,000-line `package.json`. On the Rust side all
tests are inline `#[cfg(test)]` modules at the bottom of the giant lib.rs
files (persistence's test module alone is ~7,000 lines), and
`postgres_proof.rs` spends 18,600 lines proving parity for "one low-risk
repository" — parity would be far cheaper as one trait-driven test suite
executed against both backends.

Smokes are genuinely valuable here (they are end-to-end proofs and the ADRs
reference them), but they are the *only* layer. The pure logic that most
deserves fast unit tests — route parsing in service-host, the snake/camel
mapping layer, config normalization — has none.

## 6. brain-island became the application, not the brain

AGENTS.md defines brain-island as the "pi Agent/LLM boundary," but it now
contains the service host, a hand-rolled `node:http` router (28 path-match
branches in service-host.ts), admin control/diagnostics APIs, tool registry,
runtime config, curator mutations, scheduler reads, and memory-space APIs —
204 files — and it **depends on** adapter-den, adapter-mcp, and
adapter-telegram. That inverts the intended shape where adapters are peer
boundaries. A composition-root package should depend on brain-island *and*
the adapters, not the other way around.

Related cleanup: `ts/packages/core-bridge` is dead code — nothing in the
workspace depends on it.

## 7. Documentation drift

README and AGENTS.md both state "TypeScript owns LLM calls" while
`crates/brains/openai-responses` is a ~2,800-line Rust LLM client wired into
production wake handling. The `temp/` design notes
(`rusty-crew-modular-brain-architecture-2026-06-22.md`) suggest this was a
deliberate direction change — it deserves an ADR and an Architecture Soul
update, because AGENTS.md is exactly the file agents trust first. Smaller
note: `reqwest` with `blocking` in workspace deps locks in the sync model
finding 2 complains about.

## What is working well (keep it)

- The brain/body split with frozen snapshots and structured `BrainAction`
  batches is a sound, testable coordination model; `CoreEngine::initialize`
  shows clean composition with event-recorder hydration.
- `governance/ownership.toml` plus a smoke to enforce it is a great, rare
  practice — extend it rather than replace it.
- JSON-string FFI over napi was *measured* (ADR 0002) rather than assumed.
- ADR discipline, explicit degraded-observability policy, and whitelisted
  dynamic names in persistence are all right for a coordination substrate.

## Priority order

1. Bridge codegen or runtime validation of contracts.
2. Unblock the Node event loop for Rust-brain wakes.
3. Decide product-data ownership; split the persistence monolith along the
   `repositories.rs` seams.
4. TS unit-test harness plus smoke reorganization.
5. Extract the composition root from brain-island.
