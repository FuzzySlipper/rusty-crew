# Runtime Activity Census

The runtime activity census is Rusty Crew's operator view of work that is
actually executing. It supplements session projections and the transitional
TypeScript `inFlightWakes` map; neither of those is authoritative evidence that
a provider loop, tool, process, browser, or external runtime turn is still
alive.

The public admin read is:

```text
GET /v1/admin/diagnostics/activities
```

The default census compares activity with the service's transitional session
projection, including in-flight wakes. Operators can independently compare
against only the persisted Rust session state with:

```text
GET /v1/admin/diagnostics/activities?sessionProjection=durable
```

This read-only comparison is useful when the visible service projection and
durable session state disagree. It does not mutate either projection.

It is also embedded at `runtime.activities` in the aggregate admin diagnostics
response. The capability is discoverable through the generated admin
capability catalog and OpenAPI artifact.

## Ownership And Lifecycle

Rust owns the durable activity ledger, identity validation, topology
reconciliation, restart interruption, elapsed-time calculations, and finding
codes. Native Chat Completions and OpenAI Responses loops register their wake,
provider, and tool activity directly. TypeScript reports only boundary work
through narrow bridge operations:

- wake dispatch;
- local subprocess execution;
- browser resource lifecycle.

Active external Codex app-server turns are projected from the Rust-owned
external-runtime state into the same census. On Linux, a bounded scan of the
service process tree supplies fallback evidence for subprocesses that escaped
normal instrumentation. Nested descendants are attributed to the tracked root
process rather than reported as separate false alarms.

The normal topology is:

```text
dispatch:<wake-id>
  wake:<wake-id>
    provider:<wake-id>
    tool:<wake-id>:<call-id>
      subprocess:<wake-id>:<call-id>:<pid>
      browser:<session-id>:<launch-id>
```

External runtime turns use `external:<request-id>`. Every durable record also
carries the service instance, owner, agent/profile/session identity when
available, current phase, timestamps, revision, and bounded operational
metadata.

## Reading The Census

The response contains:

- `active`: reconciled active activity with `elapsedMs` and
  `sinceProgressMs`;
- `recentlyAbnormal`: bounded failed, cancelled, and restart-interrupted
  terminal activity;
- `findings`: stable reason codes for projection and lifecycle disagreement;
- `summary`: active, abnormal, finding, and untracked-process counts;
- `serviceInstanceId`: the process incarnation that owns new records;
- `automaticCancellationEnabled`: always `false`.

Current finding codes are:

| Code | Meaning |
| --- | --- |
| `session_projection_mismatch` | Activity identity disagrees with the referenced session, the session is missing, or execution is active while the selected session projection is idle. |
| `untracked_native_run` | Native live evidence exists without a durable ledger entry. |
| `detached_dispatch` | A dispatch has no active child wake, or an activity has lost its parent. |
| `orphan_tool_execution` | A tool, process, or browser remains active without its parent activity. |
| `stale_ledger_entry` | Rust says a brain activity is active but the native run registry does not. |
| `stalled` | No progress was observed within the diagnostic threshold. |
| `restart_interrupted` | A prior service instance stopped before closing the activity. |
| `untracked_service_process` | A direct child process exists without a matching activity record. |

Findings degrade diagnostics so operators can notice disagreement. They do not
change routing, terminate work, or rewrite session state.

## Stall And Stop Policy

The default stall threshold is five minutes. It is an observation threshold,
not a wake ceiling: a healthy hour-long turn may remain active, and the census
must never cancel it automatically. `automaticCancellationEnabled` is
therefore explicit in the wire response.

Use the existing guarded session/agent emergency-stop controls when an operator
decides work must end. Do not turn a census finding into an implicit kill path.

## Restart Semantics

At bootstrap, Rust gives the process a new service-instance id and atomically
marks active records from prior instances as `interrupted` with reason
`restart_interrupted`. SQLite and PostgreSQL implement the same typed
repository contract. The restart operation scans the complete active set in a
single transaction rather than inheriting the bounded diagnostics page size.
This preserves evidence that work ended at restart while preventing stale rows
from masquerading as live execution.

## Privacy Boundary

Runtime activity storage is deliberately metadata-only. It must never contain:

- prompts or conversation bodies;
- tool arguments or tool results;
- credentials, authorization headers, or provider payloads;
- complete shell command lines;
- browser page content.

Summaries, phases, provider/model labels, tool names, process ids, and bounded
debug-detail references are allowed. Linux fallback inspection reads only the
short process name from `/proc/<pid>/comm`; it does not inspect command lines or
environments.

## Operator Check

On the disposable debug service:

```bash
curl -fsS http://127.0.0.1:9348/v1/admin/diagnostics/activities | jq .
```

During a tool-using turn, use the parent ids to confirm that dispatch, brain,
provider, tool, and process activity form one coherent tree. A session that
looks idle while this endpoint reports an active wake is a
`session_projection_mismatch`, not evidence that no work is running.
