# Architecture Review — Post-Remediation State (2026-07-06)

Status: point-in-time review snapshot. Findings reflect the working tree at
HEAD `22b6517` on 2026-07-06. Code and tests are implementation truth; when a
finding here no longer matches the code, the code wins. Supersedes
`docs/historical/architecture-review-2026-07-05.md`, which reviewed HEAD
`4dd0199` immediately before the remediation wave.

Method: single-pass re-measurement of every finding in the 2026-07-05 review
(CI, bridge, persistence, composition layer, tests, history, ops), plus a
fresh look at what changed in between. Line counts, file counts, and test
counts below were measured, not estimated.

## Overall verdict

The remediation wave was real and hit the highest-leverage targets first. Of
the seven ranked risks from 2026-07-05, three are resolved outright (no CI,
ops hardening, externalized commit history), two are substantially mitigated
with honest residuals (bridge drift, Postgres durability), and two remain
open with active plans (composition-layer decomposition, test pyramid). The
cultural fix the last review asked for — a mechanism that stops documented
intent and code from drifting apart — now exists: `verify:offline` runs the
same gate locally and in GitHub Actions on every push and PR, and the
governance checkers are no longer honor-system.

The remaining risk profile has shifted from "nothing pushes back" to "two
large decompositions are mid-flight and one new god-file is forming faster
than the old ones are shrinking."

## Scorecard against the 2026-07-05 findings

### Resolved

1. **No CI → resolved.** `.github/workflows/offline-ci.yml` runs two jobs:
   `verify-offline` (fmt, clippy `-D warnings`, `cargo test --workspace`,
   typecheck, unit tests, formatting, and the boundary / config-parity /
   cassette / four bridge-parity smokes, with `RUSTY_CREW_BRIDGE_VALIDATE=1`)
   and `verify-postgres-backend`, which runs the previously-never-run ignored
   Postgres tests against an ephemeral `postgres:16` service container. The
   local command and CI run the identical `verify:offline` script.
2. **Toolchain unpinned → resolved.** `rust-toolchain.toml` (1.96.0 with
   rustfmt+clippy), `.nvmrc`, `engines`, and CI installs the pinned npm.
3. **Ops gaps → resolved.** systemd units now use `Type=notify` with
   `WatchdogSec=45s`, fed by `service-host/src/systemd-notify.ts`. Backup is
   automated: `ops/scripts/rusty-crew-backup.sh` (pg_dump/SQLite modes) with
   `rusty-crew-backup.timer` and a debug-service twin. Stale-lock crash loops
   are fixed properly: lock reclaim validates the recorded pid is alive *and*
   looks like a Rusty Crew service before reclaiming
   (`service-config.ts:496-501`).
4. **Institutional memory → resolved going forward.** 6 of the last 100
   commit subjects are bare Den IDs (was 86 of 100); subjects are now
   descriptive.
5. **Persistence `lib.rs` god-file → resolved.** 17,443 lines → 364. Public
   contract types live in `contracts.rs`, SQLite is split across
   `sqlite_schema` / `sqlite_store` / focused `sqlite_*` support modules and
   `repos/*`, and the 7,135-line integration-test module moved out to
   `sqlite_integration_tests.rs`.
6. **`core-bridge-node` monolith → resolved.** `lib.rs` 6,835 → 261 lines;
   the napi surface is split into `wire_types`, `registries`, `binding_*`,
   `scheduler`, `storage_admin`, and `responses` modules with a dedicated
   tests module.
7. **Barrel `index.ts` → resolved.** 1,092 lines → 27; it now only re-exports
   `local-brain` plus twelve explicit `package-surface/*` modules, and an
   entrypoint surface guard smoke pins the shape.
8. **Stale-binary hazard → mitigated.** `manifestVersion` is now asserted at
   binding load with a rebuild instruction (`native-bridge/src/index.ts:2492`)
   and `native/index.d.ts` is committed alongside the `.node` binary. The
   17 MB binary itself is still in git (see residuals).

### Substantially mitigated, honest residuals

9. **Bridge drift (was risk #2).** The review offered two paths — finish
   codegen or add parity checks — and the parity-check path was taken.
   `core-bridge-codegen` now enforces, in CI: operation-name parity across
   manifest ↔ `OPERATION_NAMES` ↔ TS contracts (`check-contracts`, 122
   operations), manifest coverage of every generated napi `*Json` method
   (`check-native-surface`), Rust-emitted fixture drift (`check-fixtures`,
   10 fixtures), and a SHA-256 wire-shape fingerprint mirrored in a committed
   file and a TS export (`check-fingerprint`). Typebox validation grew from
   ~20 to 54 operation schemas and runs in CI.
   **Residual:** parity is enforced at operation granularity, not field
   granularity. Field-shape evidence exists only for the 10 fixture families
   and 54 validated schemas out of 122 operations; runtime validation is
   still opt-in (`RUSTY_CREW_BRIDGE_VALIDATE === "1"`) outside CI; and
   `native-bridge/src/index.ts` — the hand-written `Raw*`/mapping layer — has
   *grown* to 6,151 lines. A forgotten optional-field mapping in an uncovered
   type still compiles cleanly and silently drops the field.
10. **Postgres backend (was risk #3).** All three durability failures are
    fixed: a real versioned migration chain exists
    (`postgres_backend/schema_migrations.rs`, `POSTGRES_SCHEMA_VERSION = 16`
    vs SQLite's 31), the single non-reconnecting `Mutex<Client>` is replaced
    by a connection pool with tracked reconnect attempts/successes
    (`postgres_backend/pool.rs`), and the 21 `#[ignore]` tests now actually
    run in CI against an ephemeral container via
    `tools/postgres-backend-conformance.mjs`.
    **Residual:** `postgres_backend.rs` is still a 14,180-line file (down
    from 19,124) mid-split into seven submodules, ~two full SQL dialect
    implementations still exist with test-level (not compiler-level) parity,
    and the stack remains fully blocking/synchronous.

### Still open

11. **Composition layer (was risk #4) — improved but the boundary claim is
    still aspirational.** `service-app.ts` shrank 15,382 → 11,728 lines as 19
    `service-*` modules were extracted (route groups, background loops,
    runtime config), but it still lives in brain-island with 114 imports,
    and `service-host` is still a thin 367-line package — the README's
    "HTTP listener, route wiring, and drain-loop ownership" description of
    `service-host` remains untrue. The active plan (task 4242,
    `docs/service-composition-decomposition-plan.md`) is honest about this.
12. **Test pyramid (was risk #5) — least movement.** TS unit tests: 2 files →
    3 (though `service-routes.test.ts` is a substantial 1,244 lines) against
    ~116 non-smoke src files. The 132 smoke scripts still sit in
    `brain-island/src/` (39,019 of 103,125 LOC, ~38%) despite the documented
    relocation rule — unchanged since the last review. The cassette
    infrastructure now exists (`fixtures/external-cassettes/` + offline
    smoke) but holds exactly **one** cassette (Den successor-gateway
    conversation readback); provider and Rusty View behavior still has no
    recorded evidence. Root `smoke:` aliases grew 121 → 130 against the
    README's own instruction. Rust tests: 259 `#[test]` functions
    (`core-session` and `core-body` still have zero).

## New and carried-forward risks, ranked

### 1. `service-roleplay-routes.ts` is the next god-file, forming now

4,297 lines (130 KB) and the fastest-growing file in the repo — the five most
recent commits are all roleplay features landing into it. The 2026-07-05
review predicted roleplay/narrator would become the next god-file unless it
got its own package; it is happening inside the platform package while the
extraction plan is still queued behind the service-app decomposition. The
structural cycle it rides on also persists: `index.ts` re-exports
`package-surface/roleplay.ts` → `narrator-brain.ts`, while `narrator-brain.ts`
(and 11 other non-smoke modules) import from `./index.js`.

### 2. Bridge field-level drift (residual of old #2)

As measured above: 54/122 operations validated, 10 fixture families,
validation default-off outside CI, and the hand-written mapping layer growing.
The gates that exist are good; the gap is coverage, and nothing ratchets it —
unlike the production-fake allowlist, the schema/fixture counts are not
pinned, so coverage can silently stagnate while the operation count grows.

### 3. Two large decompositions in-flight simultaneously

`postgres_backend.rs` (14.2k lines, plan #4327) and `service-app.ts` (11.7k
lines, plan 4242) are both mid-split with active plan docs. Both plans are
credible, but until they land, the two files that change most often are also
the two hardest to review, and the refactor safety net is still smoke-heavy
(see risk #4). Finishing beats starting anything new here.

### 4. The boundary checker still can't see the worst boundary violations

`check-ts-package-boundaries.mjs` still skips any path containing `/smoke-`
(line 133), and 12 brain-island smokes still import `service-host` directly —
an undeclared-dependency package cycle that bypasses the adapter ports. The
smoke-relocation rule that would fix this has now survived two reviews
unexecuted while new smokes keep landing in `src/`.

### 5. Engine/persistence coupling (carried forward, unchanged)

`CoreEngine` still holds the concrete `CoreCoordinationStore` enum — no trait
boundary, so the engine still cannot be unit-tested against a fake store.
`EngineConfig`/`ClockConfig`/`EngineStorageConfig` still live in
`core-protocol` rather than `core-config`, and the injectable-clock bypass at
`core-engine/src/lib.rs:2317` (`SystemTime::now()` in
`next_queued_message_id`) is still there.

### 6. Residual hygiene items

- `core-bridge-mock` has zero consumers anywhere in the workspace — retire it.
- The 17 MB `.node` binary is still committed (now safe-at-load, but still
  repo weight on every rebuild commit).
- `adapter-den` successor-gateway still hardcodes `/v1` paths with no version
  negotiation (the memory client, by contrast, models `v1` vs
  `den-memories-v0`).
- The embedded OpenAI Responses buffered-run host still lives in the bridge
  crate behind a process-global `OnceLock<Mutex<HashMap<...>>>`
  (`core-bridge-node/src/responses.rs:252`) rather than in the brain crate or
  behind engine-owned state.
- Owner-only (`-rw-------`) permissions persist on several docs and one smoke,
  which will surprise a fresh checkout.
- Config shape is still duplicated between `service-runtime-config.ts`
  (2,311 lines) and `core-config` (2,996 lines); acceptable now that the
  parity smoke is CI-enforced, but still two copies to edit.

## Prioritized remediation

1. **Extract roleplay now, ahead of schedule.** Give roleplay/narrator its
   own package (or at minimum its own directory with a boundary rule) before
   `service-roleplay-routes.ts` doubles again; it is the only actively
   *worsening* finding in this review.
2. **Add a coverage ratchet to the bridge gates.** Count-pin validated
   schemas and fixtures the way production-fake exceptions are pinned, so
   new operations fail CI unless they ship with a schema; flip
   `RUSTY_CREW_BRIDGE_VALIDATE` default-on outside production.
3. **Finish the two in-flight decompositions before starting new ones**
   (postgres_backend #4327, service-composition 4242) — landing them
   collapses risks #3 and #4's blast radius and makes the README's
   `service-host` claim true.
4. **Execute the smoke relocation rule** (two reviews old): move smokes out
   of `src/`, declare their real dependencies, remove the checker's
   `/smoke-` skip, and stop the alias growth by deleting root aliases in
   favor of the runner.
5. **Grow the cassette library** past one file: one recorded cassette per
   external integration (Den gateway families, provider responses, Rusty
   View), captured during the live certifications that already run.
6. **Put a trait between engine and store** as the persistence split
   completes (the enum's match arms are already thin), move engine config
   types to `core-config`, and route `next_queued_message_id` through the
   injected clock.
7. **Delete `core-bridge-mock`.**

## Closing observation

Last review's closing line was that the system "does not push back when new
code widens the gaps." That is fixed — CI now pushes back, and the fastest
gaps (ops, Postgres durability, history, toolchain) closed within a day of
being named. What CI cannot do is finish migrations: the smoke-relocation
rule, the service-host ownership claim, and the bridge coverage gap are all
policy-complete and execution-stalled, and the roleplay surface is growing in
the exact shape this codebase has already had to decompose three times.
The next review should expect the two in-flight splits landed, roleplay
contained, and a bridge-coverage ratchet — those three would move the
realized share of the intended architecture from roughly 80% to roughly 95%.
