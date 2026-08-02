# Task 6514 Yielded Delegation Live Certification

Date: 2026-08-01 (America/Los_Angeles)

## Debug SQLite Target

- Service: `rusty-crew-debug.service`
- API: `http://127.0.0.1:9348`
- Storage: SQLite at
  `/home/system/rusty-crew-debug/data/engine/coordination.sqlite3`
- Provider: `deepseek-flash` (live Chat Completions inference)
- Evidence packet:
  `/home/system/rusty-crew-debug/evidence/task-6514/msb75g61/live-provider-results.json`

The parent called `fan_out_subagents_md` before its first work-quantum yield,
then completed four unrelated local tool calls. Rust admitted exactly two
delegated workers before scheduling the continuation. Both workers ran real
provider-backed turns and submitted distinct completion markers. The resumed
parent consumed both Rust-owned completion packets and finished the same
logical turn.

```json
{
  "parentSessionId": "task-6514-parent-msb75g61-session",
  "workerRunsCreated": 2,
  "completionPacketsCreated": 3,
  "continuationCount": 2,
  "childMarkers": [
    "TASK_6514_CHILD_A_MSB75G61",
    "TASK_6514_CHILD_B_MSB75G61"
  ]
}
```

The parent event stream contained a successful `fan_out_subagents_md` result,
`logical_turn_yielding`, both delegated completion markers, and one completed
terminal. Both delegated sessions advanced beyond their initial cursor.

The certification temporarily set the debug-only Chat Completions work quantum
to four tool rounds so the boundary was deterministic. Cleanup deleted all
three disposable profiles, restored the configured value to 64, and restarted
the debug service. The production service on port 9347 was not changed.

## Deterministic Support

The native SQLite integration additionally proves that yielded actions are
admitted before continuation, exact action replay creates no duplicate worker,
child completion remains visible after engine restart, and a rejected action
fails the wake with stable `brain_action_rejected` diagnostics. The shared
worker-lifecycle repository conformance suite runs the same persistence
contract against SQLite and PostgreSQL.
