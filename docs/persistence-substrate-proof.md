# Persistence Substrate Proof

Status: Implementation note for task 2879

Date: 2026-06-20

## Purpose

The persistence layer now has unit coverage for individual tables and APIs, but
Rusty Crew needs proof that those pieces behave as one runtime substrate after a
restart.

The engine test
`multi_agent_restart_search_queue_and_query_apis_prove_persistence_substrate`
is the first broad proof.

## Scenario Covered

The test uses real engine and store APIs to:

- create three full agents;
- create one delegated session through a brain action;
- persist routed messages across parent, reviewer, observer, and delegated
  agents;
- persist delegated tool-call telemetry;
- deliver and persist a completion packet;
- persist session configuration constraints and tool profile resolution;
- persist one expired queued message and one still-fresh queued message;
- restart the engine from the same data directory;
- hydrate full and delegated sessions;
- project body state from persisted bus history;
- run explicit queue expiry maintenance after restart;
- verify expired queue state is inspectable but no longer pending;
- verify search results for message history and expired queue history;
- verify runtime counters by runtime and delegated-session scope;
- verify typed query APIs for sessions, messages, completions, and worker runs;
- verify hot query-plan checks still use indexes.

## Boundary

The proof intentionally does not import pi-crew or Hermes data. It proves that
Rusty Crew's own runtime state can survive restart and remain queryable through
typed APIs. Migration tooling can build on this later through the import
metadata from task 2878.

## Queue Safety

The test keeps queue expiry explicit. Restart does not redeliver expired work
or silently purge it. The caller runs `run_maintenance` with a timestamp, after
which the expired row is terminal and searchable for inspection while the fresh
row remains pending.
