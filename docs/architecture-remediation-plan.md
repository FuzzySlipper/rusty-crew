# Architecture Remediation Plan

Implementation plan for the tensions identified in
`architecture-review-2026-07-01.md`. Phases are ordered so each one reduces
risk for the next; workstreams inside a phase can proceed in parallel across
agent lanes. Every phase ends with the existing gates green:
`cargo fmt --all --check`, `cargo test --workspace`, `npm run typecheck`,
`npm run build:native`, and the smokes named in each phase.

Guiding constraint throughout: **behavior-preserving refactors first, then
decisions, then new mechanisms.** No phase below changes wire shapes and
coordination semantics at the same time.

---

## Phase 0 — Decide and record the two open principles

Cheap, blocking, and everything downstream depends on it. Two ADRs:

**ADR: Rust brain modules.** The Architecture Soul ("TypeScript owns LLM
calls") no longer matches `crates/brains/openai-responses` being wired into
production wake handling. Write the ADR that either (a) blesses Rust brain
modules behind the language-neutral wake/stream contract as the target state,
with TS brains remaining supported, or (b) declares the Rust brain an
experiment with an explicit sunset. Update the Architecture Soul section in
AGENTS.md/README and the lane table to match. The `temp/` design notes from
2026-06-22 are most of the raw material; promote the surviving decisions into
the ADR and let `temp/` stay historical.

**ADR: durable product data ownership.** The coordination store now holds
roleplay lore, conversation branches/variants, attachments, data-bank scopes,
profile memory, and simple KV. Decide explicitly: Rust-owned durable product
storage is now in scope (extending ADR 0018/0019), or it is not and those
tables have a different home. The rest of this plan assumes the answer is
"in scope, but partitioned" — coordination state and product/memory state
live in the same process but behind separate store types and schema
namespaces — because that matches where the code has already gone. If the
decision differs, Phase 3 changes shape; nothing else does.

Also in this phase: amend `governance/ownership.toml` conventions so the
boundary smoke can eventually check *storage scope* (which crate may own
which schema/table prefix), not just crate dependencies. Even a declared-but-
unenforced list is useful input to Phase 3.

Exit criteria: two ADRs merged; AGENTS.md Architecture Soul updated; no code
changes.

---

## Phase 1 — Unblock the Node event loop (smallest correct fix first)

Lane: bridge-native + rust-brain-module. Independent of every other phase;
do it early because it is a live operational defect, not a refactor.

1. **Move the blocking brain run off the main thread.** Convert
   `run_openai_responses_brain_json` to a napi async task (napi-rs
   `AsyncTask` or `tokio` + async fn — pick one and record it; `AsyncTask`
   avoids introducing a runtime into an otherwise sync bridge). The Rust
   side may keep `reqwest::blocking` internally for now since it runs on a
   worker thread; the contract change is only that the JS-visible function
   returns a real promise.
2. **Fix the cosmetic async in the mapping layer.** The
   `runOpenAiResponsesBrain` wrapper in `native-bridge` already returns a
   promise; verify no caller depends on same-tick completion. Smoke:
   `smoke:brain` plus a new smoke that fires an admin HTTP request *during*
   a fake-client brain run and asserts it is served before the run ends —
   that is the regression test for the actual defect.
3. **Incremental delivery (second step, separable).** Today the wake stream
   is fully materialized before returning. Reuse the existing drain
   discipline: the async brain run appends `BrainWakeStreamItem`s to a
   Rust-side buffer keyed by wake id; expose `drain_brain_wake_stream(wake_id)`;
   the TS brain module polls it on its existing interval cadence. This keeps
   the "boring bridge / no callbacks into JS" posture (ADR 0002 territory)
   while enabling mid-run visibility. A ThreadsafeFunction *signal-only*
   nudge (no payload) can replace the polling interval later if latency
   demands it; measure before adding it.
4. **Follow-up:** once async delivery exists, migrate the brain crate from
   `reqwest::blocking` to async reqwest so the worker thread is not pinned
   per concurrent wake. This is capacity work, not correctness work; defer
   until multiple concurrent full agents are actually exercised.

Exit criteria: event loop provably responsive during a brain wake (new
smoke); no wire-shape changes beyond the one function's async-ness.

---

## Phase 2 — Make the bridge contract single-source

Lane: contract-steward. This is the highest-leverage investment and it
de-risks every later refactor, because after this phase a drifted field is a
build failure instead of a runtime `undefined`.

Sequence deliberately — validation first, generation second:

1. **Runtime validation at the boundary (fast win).** The mapping layer's
   `JSON.parse(...) as unknown` sites are the enforcement point. Introduce
   typebox (already used by adapter-mcp) schemas for the highest-traffic
   operations first — wake request/stream, action batch, event envelope,
   session/body state — and validate in `native-bridge` behind an env flag
   (`RUSTY_CREW_BRIDGE_VALIDATE=1`), on by default in smokes, off in
   production until confidence is built. This immediately converts silent
   drift into loud smoke failures without touching Rust.
2. **Choose the source of truth for generation.** Options: (a) the manifest
   TOML grows full type definitions, (b) Rust types annotated with
   `schemars`/serde derive emit JSON Schema, (c) contracts stay hand-written
   but a *checker* diffs them against Rust-emitted schema. Recommendation:
   (b) with a checker step first — Rust is already the authoritative side,
   serde attributes already encode the wire casing, and a checker can land
   incrementally per operation family without a flag-day.
3. **Grow `core-bridge-codegen` into that checker, then the generator.**
   Stage 1: emit JSON Schema from `core-protocol`/`core-bridge-api` types,
   compare against typebox schemas in CI (`npm run smoke:contract-drift`).
   Stage 2: generate the typebox schemas and the snake/camel mapping
   functions, deleting hand-written mapping code family by family. The
   manifest keeps its role as the *operation inventory* and gate ("may this
   operation exist"), which is what "boring bridge contracts" actually
   protects.
4. **Shrink the surface while generating.** Each operation family ported to
   codegen is the natural moment to ask whether it belongs on the bridge at
   all. Product-data CRUD verbs (attachments, KV, lore) should be reviewed
   against the Phase 0 product-data ADR — some may collapse into fewer,
   coarser operations. Target: meaningfully fewer than 97 operations, each
   one deliberate.

Exit criteria: contract drift is CI-detectable for all operation families;
mapping layer hand-code shrinking rather than growing; dead
`ts/packages/core-bridge` package deleted (nothing depends on it — verify
and remove in this phase since contract-steward owns that directory).

---

## Phase 3 — Partition the persistence monolith

Lane: rust-persistence. Do this after Phase 2 has validation in place, so
any accidental behavior change in store methods that back bridge operations
gets caught by schema checks and smokes.

1. **Mechanical split along the existing seams.** `repositories.rs` already
   names the repository groups. Convert the descriptive catalog into real
   modules: `core-persistence/src/repos/<group>.rs`, each owning its tables'
   DDL, its `impl` block (Rust allows multiple inherent impl blocks on
   `CoordinationStore` across modules, so the public API does not change),
   and its `#[cfg(test)]` module. lib.rs shrinks to store construction,
   migration ordering, shared helpers, and re-exports. This is
   behavior-preserving; land it in several PRs, one repository group each,
   with `cargo test --workspace` as the gate.
2. **Separate coordination from product storage types.** Per the Phase 0
   ADR: introduce a second facade (e.g. `ProductStore` or `MemoryStore`)
   over the product-shaped repository groups (lore, branches/variants,
   attachments, data banks, profile memory, KV), leaving
   `CoreCoordinationStore` with sessions, events, delegation, completion
   packets, counters, scheduler state. Same database file at first —
   this is a *type-level* partition that makes the boundary reviewable and
   lets `ownership.toml` gain a storage-scope rule. Physical separation
   (second SQLite file / schema) is a later, optional step driven by backup
   and migration needs, not by this plan.
3. **Replace `postgres_proof.rs` with a backend-parametric test suite.**
   Define the per-repository test suites as functions generic over a
   backend handle (or a small trait covering open/migrate/txn), run each
   suite against SQLite always and against Postgres behind the existing
   `postgres-proof` feature. The 18,600-line proof file becomes a thin
   harness plus shared suites, and every future repository gets parity
   coverage for free instead of by copy-paste. This folds into step 1
   naturally: as each repo group moves into its module, its tests move into
   the shared-suite form.
4. **core-engine internal modules.** Same treatment, lighter touch: split
   `core-engine/src/lib.rs` into `delegation.rs`, `fan_out.rs`,
   `scheduler.rs`, `wake.rs`, `actions.rs` (module boundaries per the
   existing ADR topics), keeping `CoreEngine` as the single public facade.
   No public API change.

Exit criteria: no source file in `crates/` over ~3,000 lines; parity suite
replaces `postgres_proof.rs`; storage-scope rule added to `ownership.toml`
and checked by the boundary smoke.

---

## Phase 4 — TS test harness and smoke reorganization

Lane: ts-brain + ts-adapter. Can start any time after Phase 0; sequenced
here because Phases 2–3 generate exactly the pure functions that want unit
tests.

1. **Adopt `node:test`.** Zero new dependencies, works with the existing
   tsx/tsc toolchain. Convention: `src/foo.ts` → `test/foo.test.ts` within
   each package; one `npm run test:unit` at the root running
   `node --test` across packages.
2. **First test targets (highest value per line):** the native-bridge
   mapping/casing functions (against fixtures captured from the real bridge
   — Phase 2's schemas make good generators), service-host route parsing
   (`parseProfileRegistryWriteRoute` and friends), service-config
   normalization, and profile-loading edge cases.
3. **Move smokes out of `src`.** `ts/packages/*/smokes/` (excluded from the
   package build), and replace the 109 `smoke:*` package.json entries with a
   single runner: `npm run smoke -- <name>` that discovers smoke files by
   path, plus `npm run smoke -- --list`. Keep a small curated set of
   named aliases for the ones AGENTS.md documents (`smoke:den`,
   `smoke:brain`, `smoke:bridge-wake`, `smoke:delegated-slice`,
   `smoke:mid-turn`) so existing muscle memory and docs keep working.
4. **Classify smokes** while moving them: pure-logic smokes that need no
   environment become unit tests; env-dependent ones stay smokes and declare
   their requirements (den-router URL, native build) in a header the runner
   can print.

Exit criteria: `npm run test:unit` exists and runs in CI-equivalent gates;
`brain-island/src` contains no `smoke-*.ts`; package.json scripts count
drops to a reviewable size.

---

## Phase 5 — Extract the composition root from brain-island

Lane: ts-brain, with contract-steward reviewing package boundaries. Last
because it moves the most files and benefits from Phases 2–4 (typed
contracts, unit tests, smokes runnable from new locations).

1. **New package `ts/packages/service-host`** as the composition root: owns
   `startRustyCrewServiceHost`, the HTTP listener, admin/diagnostics route
   wiring, runtime config assembly, and the setInterval drain loops. It
   depends on brain-island, native-bridge, contracts, and the adapters.
2. **Invert the adapter dependencies.** brain-island drops its dependencies
   on adapter-den/mcp/telegram; where the brain needs adapter capability,
   it declares an interface in contracts (or a brain-island `ports.ts`) and
   service-host injects the implementation. Do this adapter by adapter;
   adapter-telegram is likely the cleanest first candidate.
3. **Split the HTTP surface by route family** during the move rather than
   after: `routes/profile-registry.ts`, `routes/admin-control.ts`,
   `routes/diagnostics.ts`, `routes/scheduler.ts`, `routes/memory-space.ts`,
   each exporting a `(request) => handled | undefined` handler the root
   router composes. The 28-branch if-chain becomes a handler table. This is
   where Phase 4's route-parsing unit tests pay off.
4. **brain-island afterwards** should be recognizably the lane AGENTS.md
   describes: profile/role assembly, pi-agent integration, brain modules,
   model-callable tools. Update the lane table and the repository-structure
   diagram in AGENTS.md, and extend `ownership.toml`'s lane rules to cover
   the new package (`service-host` may depend on everything; adapters and
   brain-island may not depend on service-host).

Exit criteria: dependency direction matches the lane table; brain-island
package no longer opens sockets or owns HTTP routes; all named smokes green
from their new homes.

---

## Sequencing summary

| Phase | Theme                          | Depends on | Parallelizable with |
| ----- | ------------------------------ | ---------- | ------------------- |
| 0     | ADRs: brains, product data     | —          | 1                   |
| 1     | Event-loop unblocking          | —          | 0, 2                |
| 2     | Contract validation + codegen  | 0          | 1, 4                |
| 3     | Persistence partition          | 0, 2       | 4                   |
| 4     | Test harness + smoke reorg     | 0          | 2, 3                |
| 5     | Composition-root extraction    | 2, 4       | —                   |

Deliberately out of scope: physical database separation, replacing the
drain/poll model with push callbacks, Postgres as default backend, and any
wire-protocol redesign. Each of those should be a measured decision after
the phases above, in the same ADR discipline the project already practices.
