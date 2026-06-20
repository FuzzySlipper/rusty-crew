# Runtime Counter Reset Tool

Status: Implementation contract for task 2906

Date: 2026-06-20

## Scope

`counter_reset` is the normal brain-tool path for inspecting and explicitly
resetting Rust-owned runtime counter projections. It is intentionally bounded to
derived counters; runtime facts remain in the event log, session records,
messages, tool-call history, and completion packets.

The first implementation supports:

- `query`, returning matching counter rows;
- `summary`, returning the named runtime summary fields for one scope;
- `reset`, zeroing matching derived counter rows.

## Scopes

Counter scope types match the persistence model:

- `runtime`;
- `agent`;
- `instance`;
- `session`.

`runtime` always maps to the single `_global` runtime row. Other scopes require
an explicit `scopeId`.

## Reset Guardrails

Reset is disabled unless the host creates the tool with `allowReset: true`.
Every reset call must also provide:

- `confirm: true`;
- `triggerType`, one of `manual`, `maintenance`, or `governance_review`;
- a non-empty `reason`.

Successful reset returns the number of rows zeroed plus a post-reset query of
the same selector. Reset does not delete rows and does not mutate other scopes.
For example, resetting runtime `messages` does not clear agent or session
`messages`.

## Rebuild Policy

Counters are derived state. When a projection is wrong, the durable repair path
is to rebuild from owned runtime facts. This task adds the stable reset/query
substrate first; a later maintenance task can add a full rebuild mode without
changing the tool's basic action shape.
