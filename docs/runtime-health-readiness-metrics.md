# Runtime Health, Readiness, And Metrics Projection

Status: implementation note for task 2950

`buildRuntimeHealthProjection` turns the full runtime diagnostics projection into service health surfaces suitable for operators, admin HTTP, process supervisors, slash status responses, and future TUI/debug clients.

The health projection is intentionally narrower than diagnostics. Diagnostics explains the runtime in detail; health answers whether the service is live, whether it is ready to accept work, what is degraded, and which small metrics are useful to expose.

## Surfaces

The projection returns:

- `liveness`: shallow process health. It is `ok` if the service can answer the probe and does not include runtime details.
- `readiness`: work readiness. It fails for blocked internal runtime state or missing diagnostics, not for every external projection problem.
- `degradedStatus`: full degraded reason-code breakdown split into internal and external domains.
- `metrics`: bounded numeric samples for health, sessions, delegations, queues, adapters, persistence, observation, and runtime counters.

## Internal Versus External Health

Health distinguishes internal routing/runtime problems from external projection or adapter problems.

Internal issues include runtime sessions, delegations, queues, persistence, tools, counters, and missing diagnostics. Blocked internal issues make readiness fail.

External issues include adapter and observation sources such as Den Channels, MCP surface health, and observation writer availability. These can degrade the operator view without making the runtime unready. For example, an unavailable observation sink should report degraded status but should not restart or fail the service by itself.

## Readiness Rule

Readiness is false when:

- an internal diagnostics issue is `blocked`;
- diagnostics inputs needed to judge runtime state are missing.

Readiness can remain true while `degradedStatus.degraded` is true. This covers partial adapter outages, observation writer failures, stale external projections, and similar operator-visible problems that should not become lifecycle authority.

## Metrics Shape

Metrics are structured samples:

```ts
interface RuntimeMetricSample {
  name: string;
  value: number;
  labels?: Record<string, string>;
}
```

The current projection emits samples such as:

- `rusty_crew_runtime_health_degraded`
- `rusty_crew_runtime_health_blocked`
- `rusty_crew_internal_health_degraded`
- `rusty_crew_external_health_degraded`
- `rusty_crew_sessions_total`
- `rusty_crew_delegations_blocked`
- `rusty_crew_queue_pending`
- `rusty_crew_adapter_channel_degraded_bindings`
- `rusty_crew_adapter_mcp_degraded_surfaces`
- `rusty_crew_observation_writer_available`
- `rusty_crew_persistence_database_bytes`
- runtime counter totals for brain turns, wakes, tools, messages, completions, and queue expirations.

The shape is JSON-first for the TS admin host. A later endpoint can render these samples as Prometheus text without changing the runtime health contract.

## Smoke Coverage

`npm run smoke:runtime-health` verifies:

- liveness stays shallow and healthy;
- external observation degradation keeps readiness true while reporting degraded status;
- missing diagnostics makes readiness false;
- blocked tool registry diagnostics make readiness false;
- adapter and observation issues classify as external health.
