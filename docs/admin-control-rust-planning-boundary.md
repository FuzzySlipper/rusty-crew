# Admin Control Rust Planning Boundary

Status: implementation note for task 4709
Date: 2026-07-08

Rusty Crew mutating admin controls should be planned by Rust before TypeScript
executes side effects. TypeScript remains responsible for HTTP envelopes, auth,
operator/audit envelopes, route dispatch, and adapter glue. Rust owns the
control plan: command kind, target identity, idempotency key, operator reason,
preconditions, stable denial codes, and ordered lifecycle actions.

## Current Migrated Family

`/new` / `new_session` is the first migrated family.

Rust entrypoint:

```text
plan_new_session_control
```

The planner accepts:

- command kind and request identity;
- target current session id;
- idempotency key when present;
- operator reason and reason code;
- loaded current-session template;
- generated new session id;
- whether a channel rebind handler is available.

It returns:

- `accepted`;
- stable denial reason such as `missing_session_id`,
  `new_session_identity_not_distinct`, `invalid_new_session_id`,
  `missing_channel_rebind`, or `unsupported_control_command`;
- precondition records with `satisfied` or `failed`;
- ordered actions: `archive_session`, `create_session`, and optionally
  `rebind_channel`.

`createNewSessionLifecycleExecutor` now requires `planNewSessionControl`.
Without that Rust plan, `/new` cannot execute. If Rust denies or omits required
actions, TypeScript returns a failed control outcome before archive/create
side-effects.

## Inventory

| Control area | Current TS surface | Rust-planning target |
| --- | --- | --- |
| `/new` archive-and-create | `new-session-lifecycle.ts`, slash command routing, admin control route | Implemented by `plan_new_session_control` |
| MCP reload | `reload-mcp-control.ts` | Plan binding target, reload reason, discovery preconditions, failure/partial policy |
| Runtime rebuild | admin control profile rebuild routes | Plan profile/session replacement and hot-swap preconditions |
| Runtime config reload/update | config control routes | Plan reload mode, idempotency, validation gates, service impact |
| Scheduler controls | scheduler tick/run/pause/resume controls | Plan job target, status transition, claim/run preconditions |
| Cleanup controls | delegated-resource cleanup executor | Plan cleanup scope, dry-run/apply mode, retention and terminal-state checks |
| Background/curator controls | curator scan/preview/approve/apply controls | Continue moving candidate approval and mutation preconditions into Rust planners |
| Shutdown | shutdown control route | Plan drain window, operator reason, allowed mode, and terminal response |

## Expansion Rule

Move one command family at a time:

1. add a Rust pure planner with stable denial codes;
2. expose it through the bridge manifest and native bridge;
3. make the TS executor require the planner;
4. keep TS side effects ordered by Rust action names;
5. add focused Rust tests plus the existing route/smoke test for that control.

Do not add permissive TypeScript fallback behavior after a family has a Rust
planner. A missing bridge method should fail closed through the normal native
bridge unavailable path.
