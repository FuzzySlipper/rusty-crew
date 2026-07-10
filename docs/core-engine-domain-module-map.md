# CoreEngine Domain Module Map

Status: active extraction contract for #5367  
Measured: 2026-07-10

## Baseline

`crates/core/core-engine/src/lib.rs` is 8,906 lines and 340,859 bytes. Production
code occupies lines 1-3,402; the inline test module occupies lines 3,403-8,906.
The store ports are already split by domain. The remaining problem is that
`CoreEngine` orchestration, private policy helpers, and nearly all engine tests
still live in one file.

The extraction keeps `CoreEngine` as the public composition facade. Domain
modules implement inherent `CoreEngine` methods directly; they do not introduce
a second engine trait, a generic context object, or a catch-all `helpers.rs`.

## Dependency Direction

```text
core-protocol / core-config / core-bus / core-session / core-body
                              |
core-persistence DTOs and repositories
                              |
engine-owned narrow store ports (existing *_store.rs modules)
                              |
CoreEngine domain implementation modules
                              |
lib.rs composition facade and public re-exports
```

Sibling domain modules may use crate-private `CoreEngine` fields and shared
crate-private types from `lib.rs`. They must not call through another domain's
public facade merely to reach a store port. Cross-domain coordination is
allowed only where the behavior itself spans domains, and the owning module is
named below.

No domain module may depend on bridge, Node, TypeScript, provider SDK, Den
adapter, or service-host types. Persistence remains behind the existing narrow
ports. Provider-specific wire behavior remains in brain crates.

## Shared Type Placement

Keep these in `lib.rs` because they define composition or public API:

- `CoreEngine`, its fields, and public crate exports;
- `ProviderStateHydration` until provider state becomes a standalone public
  engine component;
- engine handle allocation and truly engine-global construction state.

Keep a type in a domain module when only that domain uses it:

- delegation validation aggregates and worker-capacity plans in
  `delegation.rs`;
- chat cursor/read projection helpers in `chat.rs`;
- provider-state keys and TTL helpers in `provider_runtime.rs`;
- queue identifiers and time arithmetic in `body.rs`;
- GitHub gate validation in `github_gate.rs`.

Shared test fixtures go in `tests/support.rs` only when at least two domain test
modules use them. Domain-specific builders stay beside their tests. There will
be no production `helpers.rs` and no test prelude that imports the whole crate.

## Production Method Inventory

| Current lines | Target module | Assigned behavior |
| --- | --- | --- |
| 169-244 | `bootstrap.rs` | initialize, storage opening, event/session hydration, startup delegated cleanup, active roleplay reactivation |
| 245-337 | `sessions.rs` | handle/bus access, event subscriptions, create/ensure/get/list/archive session |
| 338-359, 532-580 | `body.rs` | body projection, wake preparation, follow-up queue enqueue/drain/cap/expiry |
| 360-531 | `provider_runtime.rs` | provider-state hydration, required/optional absence, replacement, clear, diagnostics |
| 581-768, 3353-3393 | `github_gate.rs` | gate suspend/consume/recovery/read/cursor and request/event validation |
| 769-893 | `brain_runtime.rs` | profile tool registration, internal routing, brain action execution, brain/external/Den event ingress |
| 894-1105 | `profile_admin.rs` | storage read models, profile registry CRUD/purge, model providers, refresh impact/plan |
| 1106-1178 | `roleplay.rs` | roleplay character, persona, session metadata/projection, and import records |
| 1179-1347 | `roleplay.rs` | lore records/layers/config/links/capture/promotion/chat layers/recall; delegates through `RoleplayLoreStore` |
| 1348-1366 | `maintenance.rs` | simple KV and maintenance operations; no new generic authority use |
| 1367-1896 | `chat.rs` | slots, variants, read model, events, branches, snapshots, transcript search/jump, attachments, data-bank scopes |
| 1897-2027 | `memory.rs` | profile/session memory, descriptors, proposals, governance, digests, compaction artifacts |
| 2028-2049 | `runtime_admin.rs` | runtime search, counters, summary, reset |
| 2050-2241 | `delegation.rs` | checkpoints, cancellation, drain/status/expiry/cleanup, parent/run lookup |
| 2242-2275 | `bootstrap.rs` | shutdown, diagnostic clock access, engine clock |
| 2285-2309, 3150-3192 | `body.rs` | queue private helpers, ISO time addition, queue IDs and clock sanitization |
| 2310-2993, 3253-3327 | `delegation.rs` | spawn/pool claim/fallback, child cleanup, invariants, lifecycle projection, fan-out policy, completion wakes, event wake scheduling, status translations |
| 2994-3149 | `chat.rs` | read limits, cursors, durable/pending event projection, selected variant and status wire helpers |
| 3193-3252 | `provider_runtime.rs` | provider alias/key derivation and update-scope validation |
| 3328-3352 | `brain_runtime.rs` | tool profile validation |
| 3394-3402 | `body.rs` | RFC3339 parsing shared by engine-clock queue calculations |

The existing `scheduler.rs` remains the scheduler implementation module. Its
inherent `CoreEngine` block and fake-backed tests stay there; queue-triggered
scheduler calls remain owned by `body.rs` or `delegation.rs` according to the
event that caused them.

## Test Inventory

The inline tests move without changing their behavioral assertions:

| Current tests | Target test module |
| --- | --- |
| body projection, route wake/no-wake, queue drain/cap/TTL/history-window tests | `tests/body.rs` |
| shutdown, ensure configured, restart roleplay reactivation, persistence open/system clock/Postgres initialization | `tests/bootstrap_sessions.rs` |
| action execution/rejection, brain event ingress, tool telemetry, Den/external ingress and observability independence | `tests/brain_runtime.rs` |
| scheduler tick | existing scheduler tests or `tests/scheduler.rs` |
| delegation spawn/retry/depth, pool capacity, checkpoint, timeout, cleanup, parent archive/drain, fan-out, tool-profile resolution, completion wakes | `tests/delegation.rs` |
| multi-agent hydration/search/queue persistence substrate | `tests/restart_hydration.rs` |
| provider refresh impact and planning | `tests/profile_admin.rs` |
| chat read model, event log, variant/slot mutation, branches, snapshots, jumps, attachments, data-bank scopes | `tests/chat.rs` |
| GitHub gate durability and stale-SHA rejection | `tests/github_gate.rs` |

The following current helper assignments are explicit:

- `save_test_message_slot`, `save_test_alternate_variant`,
  `save_test_branch`, `test_branch_write`, `test_snapshot_write`,
  `test_attachment_write`, `test_data_bank_scope_write`,
  `test_message_write`, and `chat_slot_ingest_request` move to
  `tests/chat_support.rs`;
- `spawn_delegated`, `fan_out_request`, and
  `deliver_child_completion` move to `tests/delegation_support.rs`;
- `test_engine`, `test_engine_with_data_dir`, `test_engine_config`,
  `unique_data_dir`, `session_config`, and `profile_registry_write` move to
  `tests/support.rs` only where shared;
- `assert_receiver_disconnects_after_buffered_events` stays in the bootstrap
  test module unless another module genuinely needs it.

## Staged Moves

1. `bootstrap.rs` and `sessions.rs`; establish crate-private field visibility
   and move only the relevant tests.
2. `body.rs` plus existing `scheduler.rs`, then `delegation.rs`; keep engine
   clock ownership explicit.
3. `brain_runtime.rs`, `provider_runtime.rs`, and `github_gate.rs`; provider
   SDK behavior remains outside core-engine.
4. `chat.rs` and `roleplay.rs`; use the existing store ports and roleplay domain
   operations rather than recreating persistence logic.
5. `memory.rs`, `profile_admin.rs`, `runtime_admin.rs`, and `maintenance.rs`;
   keep diagnostics as read models and simple KV narrowly scoped.
6. Finish splitting inline tests, remove obsolete imports/private helpers from
   `lib.rs`, and enable final ratchets.

Each stage must compile and test independently. Mechanical method moves should
not rewrite behavior in the same commit unless visibility exposes a real bug.

## Ratchets

The final architecture check should enforce:

- `lib.rs` at or below 650 lines and 32 KiB;
- no `#[cfg(test)] mod tests` inline block in `lib.rs`;
- no production `helpers.rs`, `utils.rs`, or `common.rs` in core-engine;
- no inherent `impl CoreEngine` block in `lib.rs` larger than 250 lines;
- each domain implementation file at or below 1,500 lines, with a stricter
  follow-up split required before increasing a ceiling;
- domain tests under `src/tests/`, with no single test file above 1,800 lines;
- no imports from bridge crates, TypeScript packages, service-host, or adapter
  crates.

The 650-line target leaves room for module declarations, public re-exports,
composition fields, construction wiring, and facade documentation. It is a
final target, not a temporary ceiling: each extraction commit should lower a
monotonic interim ceiling so new work cannot refill `lib.rs` while the campaign
is in progress.
