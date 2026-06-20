# Operator Control Plane And Observation Architecture

Status: Design contract for task 2947

Date: 2026-06-20

Related Den contract:
`den-services/agent-activity-observation-contract-2026-06-19`

## Purpose

Rusty Crew needs operator surfaces that make the runtime usable as a service:
diagnostics, health, admin APIs, slash commands, debug clients, and a TUI. Those
surfaces must not become a second runtime. They observe projected state and
request explicit lifecycle operations from Rust-owned control APIs.

This note defines the boundary later `2821` tasks should cite.

## Four Kinds Of State

Do not merge these into one ambiguous "activity" layer.

1. **Executable runtime state**

   Rust-owned facts that can affect scheduling, delivery, wake behavior,
   lifecycle, completion, or persistence. Examples: sessions, worker runs,
   wake requests, queued messages, completion packets, tool telemetry, counters,
   and session configs.

2. **Diagnostics truth**

   Read-only projections over executable runtime state and adapter/tool health.
   Diagnostics can classify health, degraded status, blockers, recent errors,
   and inventory drift. Diagnostics must use typed Rust persistence/adapter APIs,
   not raw SQLite or private adapter internals.

3. **Observation projections**

   Display-only `agent_activity.v1` events for Den Web breadcrumbs, active-work
   lanes, and agent overview panels. Observation can summarize runtime and
   adapter activity, but it must not drive wake, delivery, completion, dedupe,
   retry, or task state.

4. **Conversation rows**

   Channel messages, memberships, reactions, read cursors, and transcripts.
   Conversation rows are user-visible conversation data, not runtime truth and
   not observation truth. Channel adapters may link conversation refs into
   observation payloads, but should not make Den Web scrape prose for status.

## Authority Rules

- Rust remains the lifecycle authority for session create/archive, delegated
  cancellation, queue expiry, per-session reload requests, and shutdown/drain
  semantics.
- Admin, slash-command, debug, and TUI surfaces issue explicit control requests.
  They do not mutate persistence tables, adapter objects, or channel bindings
  directly.
- Control responses should be auditable. A successful command should identify
  what it changed and the resulting runtime IDs.
- Read surfaces should consume shared diagnostics projections. The admin API,
  `/status`, `/session`, degraded health endpoint, debug API client, and TUI
  should not each invent their own truth model.
- Den task/project/document rows remain Den-owned. Rusty Crew may store Den
  references and include them in diagnostics/observation refs.

## Slash Commands

Slash commands are control-plane commands intercepted before LLM prompting.
They are not model-callable tools and should not be listed in tool inventory.

Initial command semantics:

- `/help`: read-only command catalog and surface-specific availability.
- `/status`: read-only diagnostics summary for current runtime/session/agent.
- `/session`: read-only session/identity/config summary.
- `/new`: lifecycle control operation that archives the old session and creates
  a fresh session. It is not in-place prompt/context clearing.
- `/reload-mcp`: explicit per-session MCP reload request. It should not be a
  global hidden reset.

Slash command parsing can live TS-side if that is the channel host, but the
resulting operation must call a Rust-owned control API for lifecycle changes.

## `/new` Lifecycle Rule

`/new` means create and archive:

1. Identify the current session from the channel/profile binding.
2. Archive the old session through Rust lifecycle control.
3. Create a new session from an explicit session config.
4. Rebind the channel/conversation surface to the new session.
5. Keep old runtime history searchable according to retention policy.
6. Emit an observation breadcrumb for the lifecycle boundary.

This avoids the risky half-state where context appears fresh but old pending
queue items, tool bindings, MCP connections, or wake state still belong to the
same executable session.

## Admin API Boundary

The admin HTTP surface should be a thin host over typed control and diagnostics
APIs. It may be Rust, TS, or hybrid, but the boundary is:

- read-only endpoints return diagnostics projections;
- guarded control endpoints submit explicit control requests;
- auth and audit are mandatory for mutating operations;
- request bodies name target runtime IDs rather than selecting private objects;
- dangerous operations return structured reason codes and affected IDs.

Examples of read-only endpoint families:

- runtime overview;
- sessions and agents;
- queues and retention;
- worker/delegated run state;
- tool/MCP/channel inventory;
- health/readiness/degraded summary;
- recent observation and audit refs.

Examples of control endpoint families:

- archive session;
- create session;
- `/new` equivalent archive-and-create;
- cancel delegated session;
- request checkpoint;
- reload per-session MCP;
- run explicit maintenance.

## Diagnostics Projection

Diagnostics should be built from:

- typed runtime query APIs;
- runtime counters and summaries;
- event/search projections;
- session config snapshots;
- adapter diagnostics;
- tool registry diagnostics;
- queue and maintenance reports.

Diagnostics should classify health with reason codes such as:

- `ok`;
- `degraded_adapter`;
- `missing_binding`;
- `missing_canonical_identity`;
- `stale_session`;
- `queue_backlog`;
- `expired_queue_items`;
- `tool_registry_invalid`;
- `mcp_reload_failed`;
- `persistence_pressure`;
- `observation_unavailable`;
- `blocked_dependency`.

Diagnostics are allowed to be richer than observation. They can include debug
details for operators, but they still must avoid secrets, raw prompts, giant
tool outputs, and direct private implementation handles.

## Observation Contract

Rusty Crew should produce Den observation events shaped by
`agent_activity.v1`.

Producer rules from the Den contract:

- use `source_domain: "runtime"` for Rusty Crew runtime/session/lifecycle
  activity unless the event is explicitly delivery-owned;
- include canonical profile and concrete instance/session identity when known;
- set `payload.kind = "agent_activity.v1"` and `schema_version = 1`;
- include short `summary`, `severity`, `visibility`, `adapter`, and `surface`;
- use `work_ref` and `result_ref` handles instead of copying authoritative
  state;
- never include raw prompts, full tool output, secrets, credentials, or
  executable commands;
- fail closed or degrade visibly when required observation write config or
  canonical identity is missing.

Observation event families Rusty Crew can emit:

- session started/resumed/idle/blocked/failed/stopped;
- work started/checkpoint/waiting/completed/failed;
- long or risky tool call started/completed/failed;
- adapter connected/disconnected/degraded/recovered;
- admin or slash-command lifecycle events;
- queue maintenance summaries when operator-visible.

Observation is display-only. Consumers must not use it to create, claim, retry,
complete, fail, dedupe, route, or wake work.

## Debug Client And TUI

The debug API client and TUI should consume the same diagnostics projection as
admin and slash-command responses. They can add layout, filtering, keyboard
navigation, and local polling, but they should not query private Rust structs,
SQLite tables, or adapter internals directly.

The TUI should present at least:

- runtime health/degraded banner;
- agents/instances/sessions;
- active and delegated work;
- queue and retention state;
- tool/MCP/channel inventory summaries;
- recent observation/audit refs;
- recent errors with reason codes.

## Failure Semantics

- Missing diagnostics data should produce degraded diagnostics, not a panic.
- Missing observation write config should be visible when observation is
  required, but should not block executable runtime work unless an operator
  policy explicitly says observation is required.
- Unknown observation event types should render generically from `summary`.
- Unknown command names should return command help, not reach the LLM.
- Unauthorized or unsafe admin control requests must fail closed before side
  effects.

## Later Task Guidance

- `2948` should implement the shared diagnostics projection core.
- `2949` should implement the observation producer against this note and the
  Den activity contract.
- `2951` should define the HTTP host/auth/API details without changing runtime
  authority.
- `2954` should build a slash-command router that returns command requests and
  responses, not tools.
- `2955` should implement `/new` as archive-and-create.
- `2963`, `2964`, and `2965` should prove control flow, observation, and
  admin/debug surfaces end to end.
