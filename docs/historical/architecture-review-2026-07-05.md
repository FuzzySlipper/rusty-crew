# Architecture Review — Long-Term Maintenance & Durability (2026-07-05)

Status: point-in-time review snapshot. Findings reflect the working tree at
HEAD `4dd0199` on 2026-07-05. Code and tests are implementation truth; when a
finding here no longer matches the code, the code wins. Supersedes the scope
of `docs/historical/architecture-review-2026-07-01.md` where they overlap.

Method: five parallel deep-dive reviews (Rust core crates; Rust↔TS bridge;
brain-island; service-host/adapters/boundaries; testing/CI/docs/ops), then
cross-cutting synthesis. Line counts and test counts below were measured, not
estimated.

## Overall verdict

The macro-architecture is genuinely good — the crate/package graph is a clean
DAG, dependency inversion between brain-island and the adapters is real, and
the codebase demonstrates the right patterns (data-driven registries, typed
errors, versioned migrations, governance checkers). The durability risks are
almost all in the **middle layer**: a handful of god-files where the
architecture stops, a Rust↔TS bridge held together by four hand-maintained
type mirrors, a production Postgres backend that is mislabeled, untested, and
unable to evolve its schema, and a verification story that is 100%
honor-system because there is no CI. Several of the biggest risks share one
root cause: **the documented architecture describes an intended state the
code has not reached, and nothing mechanical closes the gap.**

## What's healthy (worth actively preserving)

- **Adapter boundary is textbook.** Ports are interfaces in
  `ts/packages/brain-island/src/service-adapter-ports.ts`, adapters depend
  only on `@rusty-crew/contracts`, and `service-host` is the only place
  concrete adapters are imported and injected. Boundary checks confirm no
  adapter↔brain-island production imports.
- **Clean Rust crate DAG, no cycles.** `core-protocol` is the leaf; the
  `openai-responses` brain crate depends only on `core-bridge-api` +
  `core-protocol` with HTTP behind a `ResponsesClient` trait — it fully
  respects the "no reaching into coordination internals" rule.
- **Rust error discipline.** Typed `CoreError` everywhere, no `anyhow`,
  effectively zero `unwrap`/`expect` in non-test code, poisoned-lock
  handling in `CoordinationStore::conn()`.
- **Data-driven tool/profile registration.** Adding a tool is one metadata
  entry in `tool-registry.ts` plus the referenced binding — the right growth
  pattern. `profile-loading.ts` is likewise data-driven.
- **Governance is machine-checkable.** `governance/ownership.toml`,
  `governance/storage-scope.toml`, and the four `tools/check-*.mjs` scripts
  are real, deterministic, offline gates (production-fake exceptions are
  count-pinned so the list cannot silently rot).
- **The critical external dependency is pinned properly**:
  `@earendil-works/pi-*` at exact `0.79.8` with the upstream commit recorded
  in `docs/pi-package-source-lock.md`. SQLite has a real 31-step versioned
  migration chain. `Cargo.lock` and `package-lock.json` are committed.
- **Doc curation exists**: `docs/historical/` moves with a README explaining
  each supersession, and most docs carry a source-of-truth posture note.

## Top risks, ranked

### 1. No CI — every gate is a manual ritual (highest-leverage fix)

There is no `.github/`, no git hooks, nothing automated. The verification
checklist (fmt, clippy `-D warnings`, ~250 Rust tests, typecheck, boundary
smokes) exists only as prose duplicated across `README.md`, `AGENTS.md`,
`agents-project.md`, and `docs/README.md`. The well-designed governance
checkers never run unless someone remembers. One skipped run lands a boundary
violation or regression silently. Everything else in this review gets cheaper
once a single workflow runs the offline gates on every push.

### 2. The Rust↔TS bridge is four hand-maintained mirrors with silent field-drop

`core-bridge-codegen` is a scaffold that emits only 5 sample fixtures — it
does **not** generate `contracts/src/index.ts` (1,655 lines, no generated-file
marker), the ~200 `JsXxx` napi structs in `core-bridge-node`, or the 38
`Raw*` interfaces + ~90 bidirectional mapping functions in
`native-bridge/src/index.ts` (5,550 lines, all hand-written). A single
protocol type exists in four hand-maintained shapes (Rust protocol struct,
flattened napi object, TS contract, TS `Raw*` wire mirror).

Adding one field to a protocol type means hand-editing 5–8 places; because
most fields are optional, forgetting a mapping **compiles cleanly and
silently drops the field**. Runtime typebox validation is off by default
(`RUSTY_CREW_BRIDGE_VALIDATE=1`) and covers only ~20 of 100+ operations. The
fixture-drift smoke checks Rust self-consistency for 5 types and never looks
at TS.

Compounding this: the 17 MB `.node` binary is committed to git while its
generated loader and `.d.ts` are gitignored, and the napi version check does
not run on the co-located-binary load path this repo actually uses — a stale
binary loads silently and fails as `undefined is not a function` at call
time. `MANIFEST_VERSION = 1` in `core-bridge-api` is never bumped or checked
across the boundary.

### 3. The Postgres backend is too monolithic and needs stronger test gates

`crates/core/core-persistence/src/postgres_backend.rs` (19,124 lines, 711
functions) is the production PostgreSQL backend for the live service
(`open_postgres()` instantiates `PostgresBackendStore`), but much of the
repository implementation still sits in one large file.

- **No repository trait**: `CoreCoordinationStore` is an enum with 119
  hand-written two-arm match dispatches in `lib.rs` (17,443 lines); ~870 SQL
  statements are duplicated across two dialects with no compiler-enforced
  parity. The only conformance "trait" is defined inside a test module.
- **Divergent migrations**: SQLite has 31 versioned, transactional
  migrations; Postgres has one `CREATE TABLE IF NOT EXISTS` blob with a
  hardcoded version stamp and literal timestamp. **Adding a column to an
  existing Postgres deployment silently does nothing.**
- **Untested in practice**: all 32 Postgres tests are `#[ignore]` and require
  a machine-specific env file; the 711-function backend never runs under a
  plain `cargo test`.
- **Availability**: the store holds a single non-reconnecting
  `Mutex<Client>` connection (no pooling); a network blip bricks persistence
  until process restart. The whole stack is blocking (sync `postgres`,
  blocking `reqwest`), with all DB access serialized through one connection
  per backend.

The backend advertised for durability is the least durable component.

### 4. The composition layer contradicts the documented architecture

README says `service-host` owns "HTTP listener, route wiring, and drain-loop
ownership." Reality: `service-host` is 208 lines (socket lifecycle + CORS +
adapter injection), and everything else — HTTP routing via a hand-rolled
if-chain, drain loops (`drainAndDispatchWakes`, Telegram outbound, Den
observation projection), scheduler ticks, lifecycle, glue — lives in
`brain-island/src/service-app.ts` at **15,382 lines** with ~449 private
top-level declarations behind one factory and 62 imports.

Related decay in brain-island (~97k LOC, 225 files, flat `src/`):

- `index.ts` (1,092 lines) is simultaneously a barrel (30 re-exports) and the
  home of core shared types, with **107 internal importers** and confirmed
  circular imports (e.g. `index.ts` re-exports `narrator-brain.ts` while
  `narrator-brain.ts` imports types from `index.js`).
- Domain features (roleplay/narrator: `narrator-brain.ts`,
  `scene-state-tool.ts`, roleplay browser API + smokes) are accreting inside
  the platform package.
- brain-island smokes import `service-host` and adapters directly as
  undeclared (phantom) dependencies, creating a brain-island↔service-host
  package cycle and bypassing the ports they exist to protect (the TS
  boundary checker explicitly skips `smoke-*` files).

Similarly, `core-bridge-node/src/lib.rs` (6,835 lines) is not thin transport:
it hosts brain/platform/subscription registries and a full embedded OpenAI
Responses execution host backed by a global mutable
`static Mutex<HashMap<...>>` of buffered runs.

### 5. Inverted test pyramid resting on infrastructure that may not exist tomorrow

- **2** TypeScript unit test files vs **264** TS src files (~0.8%).
- The de-facto suite is **136 smoke scripts** (55% of brain-island's files,
  ~37.6% of its LOC, still living in `src/` despite the documented
  relocation rule in `docs/smoke-test-inventory.md` — new smokes are still
  being added in place).
- Only ~9 smokes run offline; the rest need a native build, a running
  service, den-router at `127.0.0.1:18082`, Postgres, or live LLM providers.
  There is no single "verify everything offline" command.
- `fixtures/` is 9 files / 76 KB — config parity and bridge wire-shapes only.
  **No recorded Den, provider, or Rusty View responses exist.** If Den or a
  provider changes or disappears, the integration-proof story (including
  live-deliverable certification) collapses to ~250 Rust unit tests and 2 TS
  tests. Refactoring `service-app.ts` today has essentially no safety net.
- Rust test distribution: core-persistence 119 (SQLite covered; Postgres
  ignored), core-engine 45, openai-responses 26; `core-session` and
  `core-body` have zero tests.

### 6. Institutional memory lives outside the repo

86 of the last 100 commit subjects are bare Den task IDs (`4196`, `4192`, …)
with no bodies, and `AGENTS.md` explicitly forbids reconstructing Den state
from local files. If Den is ever migrated or lost, `git blame` returns
meaningless integers with no in-repo recovery path. The 22 ADRs and the
active `docs/historical/` curation partially mitigate this for major
decisions, but routine change rationale is externalized and non-durable.

### 7. Ops and reproducibility gaps

- **No backup automation**: `RUSTY_CREW_BACKUP_DIR` is defined in
  `service.env.example` but unused; the runbook prescribes manual
  stop-and-tar. No `pg_dump`, no timer unit, no PITR.
- **Crash-loop risk**: a hard kill leaves a stale `run/service.lock`;
  startup fails on an existing lock; systemd `Restart=on-failure` +
  `RestartSec=5s` then loops every 5 s until a human removes the lock.
- No systemd `WatchdogSec`/`Type=notify` (hangs are invisible), no log
  rotation. SQLite WAL checkpointing is a manual admin route the runbook
  already warns can fall behind.
- **No toolchain pinning**: no `rust-toolchain.toml`, no `.nvmrc` or
  `engines` field — fresh-machine rebuild of the napi module depends on the
  builder happening to have compatible Node and Rust versions.
- **Script sprawl**: 121 root `smoke:` aliases in `package.json` duplicating
  the auto-discovering `tools/smoke-runner.mjs`, against the README's own
  instruction not to add more.

### Additional medium findings

- Config shape is duplicated across `service-runtime-config.ts` (2,268
  lines) and `core-config/src/lib.rs` (2,536 lines). Rust is the validation
  source of truth and a parity smoke guards the mirror — good design — but
  nothing structurally prevents drift between manual runs, and the 62
  `RUSTY_CREW_*` env vars are parsed on the TS side only.
- Engine/persistence coupling: `CoreEngine` holds the concrete
  `CoreCoordinationStore` enum (no trait boundary), so the engine cannot be
  unit-tested without a real store. Clock is injectable (good), but
  `next_queued_message_id` calls `SystemTime::now()` directly, bypassing the
  fixed clock.
- Engine config types (`EngineConfig`, `ClockConfig`, `EngineStorageConfig`)
  live in `core-protocol`, not `core-config`, leaving `core-config`'s role
  ambiguous and making the protocol leaf heavier than a protocol crate
  should be.
- `adapter-den`: memory client versions its API (`v1` vs `den-memories-v0`);
  the successor-gateway client hardcodes `/v1` paths with no version
  negotiation.
- Boundary checkers are lint-grade (regex + declared deps): dynamic imports
  evade them, and `contracts`/`native-bridge` have no direction rules.
- `core-bridge-mock` (40 lines, 6 of ~100 operations) is likely vestigial;
  confirm consumers, then retire.
- Some docs have owner-only (`-rw-------`) permissions, which will surprise
  a fresh checkout.

## Prioritized remediation

1. **Add CI now** — one workflow: `cargo fmt --check`, clippy,
   `cargo test --workspace`, `npm run typecheck`,
   `npm run smoke:architecture-boundaries`, the runtime-config-parity and
   bridge-validation smokes. Converts all existing governance from
   honor-system to enforced. Cheapest, highest leverage.
2. **Close the bridge drift gap** — either finish `core-bridge-codegen` (its
   stated purpose) so contracts/`Raw*`/mappings are generated, or add a
   manifest↔napi↔contracts field-parity check; make typebox validation
   default-on in dev/CI and extend coverage past ~20 types. Stop committing
   the `.node` binary (or commit its loader/types with it) and assert
   `manifestVersion` at binding load.
3. **Rescue the Postgres backend** — continue splitting `postgres_backend`
   into repository modules; keep the versioned Postgres migration chain honest
   (or derive both schemas from one source); run the Postgres tests against an
   ephemeral container in CI; keep reconnection and pooling under active test.
4. **Decompose the composition layer** — move the HTTP app/router/drain
   loops from `service-app.ts` into `service-host` (making the README true),
   split the barrel `index.ts` into a types module + explicit exports to
   kill the circular imports, and give roleplay/narrator its own package
   before it becomes the next god-file.
5. **Rebalance the test pyramid** — extract pure logic from `service-app.ts`
   handlers into unit-testable modules as decomposition proceeds; record
   Den/provider cassette fixtures so integration behavior survives external
   change; execute the smoke-relocation rule and delete the 121 redundant
   root aliases in favor of the runner.
6. **Pin the build** — `rust-toolchain.toml`, `.nvmrc`/`engines`; document
   native build prerequisites (Rust + C toolchain + Node ABI).
7. **Harden ops** — automatic stale-lock reclaim on boot (validate the
   recorded pid), a backup timer (`pg_dump` + SQLite snapshot), systemd
   `WatchdogSec`, log rotation.
8. **Make history self-contained** — include the task title in commit
   subjects alongside the Den ID, so archaeology survives Den.

## Closing observation

The intended architecture is sound and about 70% realized. The remaining 30%
— bridge codegen, service-host ownership, smoke relocation, Postgres parity,
backup tooling — is documented as intent but stalled, and with no CI there is
no mechanism forcing convergence. The single most valuable cultural fix is to
stop letting documented aspiration and current code drift apart silently:
every gap above was already acknowledged somewhere in `docs/`, which means
the problems are seen — the system just does not push back when new code
widens them.
