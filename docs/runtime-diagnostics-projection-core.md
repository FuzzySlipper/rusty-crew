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
- `ownership.sections`: source ownership labels for each major diagnostics
  section;
- summary counts for sessions, delegations, queues, tools, and errors;
- sectioned diagnostics for runtime, queues, persistence, adapters, tools, and observation;
- bounded issue records with source, severity, reason code, and optional session id.

## Ownership Labels

The projection explicitly labels whether each section is durable Rust-owned
state or a TypeScript/external projection. This prevents admin clients from
treating adapter health or TS assembly metadata as coordination authority.

Current section authority families:

- `rust_coordination`: durable runtime facts read through native bridge read
  models, including `runtime.counters`, `runtime.sessions`,
  `runtime.provider_states`, `runtime.buffered_brain_runs`, `queues`, and
  `persistence`;
- `ts_service_projection`: service-host assembly/readback, including
  `runtime.brain_modules`, `runtime.pauses`, and selected tool catalog
  projection;
- `ts_adapter_projection`: adapter-owned projections such as channel and MCP
  diagnostics;
- `external_service_projection`: external dependency readbacks such as
  observation writer health;
- `not_supplied`: the caller did not supply a section. Missing required durable
  inputs produce `diagnostics_missing` instead of invented defaults.

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
