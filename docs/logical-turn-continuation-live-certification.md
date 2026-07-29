# Logical-turn continuation live certification

Task 6371 certifies the Rust-owned durable logical-turn continuation path across
both production brain protocols. The backend and live-provider portions are
complete. Rusty View browser certification remains owned by task 6370 and must
land before task 6371 is closed.

## Deterministic coverage

The ignored long-form regressions deliberately exceed the former 512-round
mitigation and run only when explicitly selected:

```bash
cargo test -p rusty-crew-chat-completions-brain \
  minimal_loop_completes_over_512_rounds_across_many_work_quanta \
  -- --ignored --exact
cargo test -p rusty-crew-openai-responses-brain \
  continuation_quantum_completes_over_512_rounds_without_duplicate_tools \
  -- --ignored --exact
```

Core-engine regressions cover the same logical turn across repeated work
quanta, SQLite restart hydration, running cancellation, queued cancellation,
and PostgreSQL restart/cancellation parity. The focused runs and the normal
workspace suite preserve exact-once tool effects and one terminal transition.

## Live debug-service proof

The dedicated smoke is pinned to the debug service and refuses the production
service root:

```bash
npm run smoke:logical-turn-continuation-live-debug-service \
  -w @rusty-crew/brain-island
```

The accepted run used port 9348 and wrote its persistent result to:

```text
/home/system/rusty-crew-debug/evidence/task-6371/ms67mdar/live-provider-results.json
```

Observed results:

| Protocol | Provider | Continuations | Provider requests | Tool rounds | Restart |
| --- | --- | ---: | ---: | ---: | --- |
| Responses | `responses-proxy-cert-5389` | 5 | 5 | 4 | yes |
| Chat Completions | `tester-chat` | 5 | 5 | 4 | no |

Both logical turns completed exactly once after four distinct successful tool
effects. The Responses turn resumed after a service restart. A separate Chat
Completions turn was cancelled from `queued_to_continue` and reached one
`cancelled` terminal without a completed terminal. Neither old continuation
limit reason code appeared.

The run also exposed and fixed two restart/scheduling races:

- restart reconciliation no longer fabricates `service_restart_interrupted`
  while Rust owns a nonterminal logical turn;
- continuation wake events consumed while the same session is in flight are
  coalesced and dispatched after that epoch releases ownership.

Startup now asks Rust to republish runnable continuation tickets only after the
service event subscription exists. Rust claim/ticket authority keeps that
operation idempotent.

The smoke temporarily set both work quanta to `1`, then restored the debug
service defaults to `64`. The production service on port 9347 was not touched.

## Finite-ceiling removal proof

Task 6372 restarted and inspected both installed services after deleting the
legacy whole-turn timeout authority and configuration surfaces:

| Instance | Storage | Loaded brain modules | Finite lifetime fields | Retired admin route |
| --- | --- | ---: | --- | --- |
| live `9347` | PostgreSQL | 11 | none | `404 unknown_admin_control_route` |
| debug `9348` | SQLite | 35 | none | `404 unknown_admin_control_route` |

The diagnostics scan checked every scalar path for the removed service,
session, profile, and Rust coordinator timeout names. The installed
`/home/system/rusty-crew/config` and
`/home/system/rusty-crew-debug/config` trees likewise contain none of the
retired fields or continuation-limit environment variables. The live service
retains three historical abnormal-activity records with the old Chat
Completions limit reason; those immutable observations are not active policy,
configuration, or a reported effective ceiling.

Every loaded module reports a disabled provider-request deadline and a
64-round scheduling quantum. A provider may still opt into a bounded request
deadline, but that deadline governs one external operation. The quantum yields
and resumes the same logical turn; neither value is a whole-turn lifetime.

Fresh-install and strict-validation coverage lives in
`smoke:turn-lifetime-clean-break`. It proves that retired service, session, and
profile fields are rejected rather than silently ignored.

## Gates

The task implementation passed:

```bash
npm run verify:ts
npm run test:postgres-backend
```

The Rust segment of `npm run verify:offline` also passed before its TypeScript
bridge ratchets identified and drove the new operation's direct validation
coverage. The final exact commit must pass GitHub jobs `Verify Offline` and
`Verify Postgres Backend`.

## Remaining browser closure

Task 6370 must render continuation count, cumulative rounds, progress, and
attention state from the generated APIs and SSE contract. Its browser proof
must demonstrate live progress through multiple yields, restart/reconnect
replay without duplicate transcript effects, and explicit operator cancel.
Screenshots and machine-readable browser evidence belong with that task.
