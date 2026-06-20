# Runtime Observation Activity Wiring

Status: implementation note for task 2962

Rusty Crew now has explicit observation wiring points for runtime activity through `RuntimeActivityObserver` in `@rusty-crew/brain-island`.

The observer is a small facade over `AgentActivityObservationProducer`. It exists so runtime, adapter, admin, and command surfaces use one deliberate activity contract instead of each hand-rolling Den Web breadcrumbs.

## Covered Activity Families

`RuntimeActivityObserver` can publish:

- session started/resumed/idle/blocked/failed/stopped;
- coarse work started/checkpoint/waiting/completed/failed;
- long/risky tool calls and all failed tool calls;
- adapter connected/disconnected/degraded/recovered.

Tiny successful tool calls are suppressed by default with `low_signal_tool_call` so observation does not become a noisy trace dump.

## Existing Wired Control Surfaces

Guarded admin control routing emits display-only admin command lifecycle events when configured:

- `admin_command_started`
- `admin_command_completed`
- `admin_command_failed`

`/new` lifecycle execution emits:

- `agent_session_stopped` for the archived session;
- `agent_session_started` for the fresh session.

`/reload-mcp` execution emits:

- `adapter_recovered` for successful reload;
- `adapter_degraded` for degraded reload.

All of these are `agent_activity.v1` payloads and remain display-only.

## Failure Posture

The underlying producer decides missing-sink behavior:

- optional producer mode skips;
- required producer mode degrades visibly with `observation_unavailable`;
- write failures degrade visibly.

Observation degradation should surface through diagnostics. It should not become runtime lifecycle authority unless an explicit operator policy says observation is required.

## Smoke Coverage

`npm run smoke:runtime-activity-observer` verifies:

- session lifecycle activity;
- coarse work checkpoints;
- low-signal tool suppression;
- long/risky tool publication;
- failed tool publication;
- adapter degraded/recovered publication;
- required missing observation sink degradation;
- compact `agent_activity.v1` payloads with runtime instance attribution.
