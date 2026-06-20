# Observation Activity E2E Proof

Status: implementation note for task 2964

Rusty Crew now has an end-to-end observation activity smoke proving it can produce Den Web-compatible `agent_activity.v1` events without breadcrumb scraping.

The proof lives in `npm run smoke:observation-activity-e2e`.

## Scenario

The smoke emits activity through a fake observation endpoint for:

- session lifecycle;
- work checkpoint;
- adapter degraded;
- adapter recovered;
- long/risky tool activity;
- admin command started;
- admin command completed.

Each event includes:

- `source_domain`;
- `event_type`;
- canonical `agent_identity`;
- `runtime_instance_id` when applicable;
- compact `agent_activity.v1` payload with `schema_version: 1`.

## Consumer Fixture

The smoke includes a small Den Web-style lane consumer fixture. It renders known events normally and unknown/future event types generically from `payload.summary`.

The fixture intentionally does not call wake, delivery, completion, or dedupe hooks. Observation is display-only.

## Boundary

This proof composes:

- `RuntimeActivityObserver`;
- `AgentActivityObservationProducer`;
- `handleAdminControlRequest`;
- `admin_command_*` observation events.

No observation event is used as runtime authority. Runtime wake, delivery, completion, and dedupe counters remain zero.

## Smoke Output

Expected smoke output includes:

- seven published events;
- event types for session, work, adapter, tool, and admin command activity;
- `unknownKnown: false` for future event rendering;
- `wakeCalls: 0`;
- `deliveryCalls: 0`;
- `completionCalls: 0`.
