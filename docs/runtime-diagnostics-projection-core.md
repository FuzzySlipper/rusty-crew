# Runtime Diagnostics Projection Core

The runtime diagnostics projection is the shared read model for admin, health, slash-command status responses, debug clients, and the TUI. It is read-only and does not become runtime authority.

## Inputs

`buildRuntimeDiagnosticsProjection` consumes typed records and summaries:

- runtime counter summary;
- session states;
- delegated session statuses;
- queue summaries;
- persistence/search summaries;
- adapter diagnostics projection;
- tool registry diagnostics;
- observation writer health;
- recent runtime errors.

Callers are responsible for obtaining these inputs through typed Rust bridge/persistence APIs or adapter diagnostics APIs. The projection does not inspect SQLite, Den product data, adapter internals, prompts, credentials, or tool payloads.

## Health Model

The projection returns:

- `health`: `ok`, `degraded`, or `blocked`;
- `reasonCodes`: stable reason codes for admin/health/TUI consumers;
- summary counts for sessions, delegations, queues, tools, and errors;
- sectioned diagnostics for runtime, queues, persistence, adapters, tools, and observation;
- bounded issue records with source, severity, reason code, and optional session id.

Reason codes include:

- `stale_session`
- `queue_backlog`
- `expired_queue_items`
- `degraded_adapter`
- `mcp_reload_failed`
- `tool_registry_invalid`
- `persistence_pressure`
- `observation_unavailable`
- `blocked_dependency`
- `recent_runtime_error`
- `diagnostics_missing`

## Failure Semantics

Missing diagnostics inputs produce degraded `diagnostics_missing` issues instead of throwing. Blocked delegation or invalid tool registry issues raise the aggregate health to `blocked`; other issues usually make the projection `degraded`.

Observation state remains display-only. Observation writer failures should be visible in diagnostics but must not block executable runtime work unless a future explicit policy says otherwise.

## Verification

Run:

```bash
npm run smoke:runtime-diagnostics
```

The smoke proves healthy, degraded/blocked, and missing-input projections.
