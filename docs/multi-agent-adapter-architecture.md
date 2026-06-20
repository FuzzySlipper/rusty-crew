# Multi-Agent Adapter Architecture And Isolation Boundaries

Status: Design contract for task 2926

Date: 2026-06-20

Related docs:

- `operator-control-plane-and-observation-architecture`
- `tool-architecture-registry-rules`
- `runtime-state-query-apis`

## Purpose

Rusty Crew may run many full/prime agents inside one service. Each agent can
have independent channel activity, channel memberships, Den references, MCP
connections, and selected tool surfaces. Platform adapters must scale with that
model without becoming the runtime bus or a global singleton.

This note defines adapter boundaries for Den Channels, MCP, Telegram, and future
external integrations.

## Core Rule

Adapters translate external protocols. Rust owns coordination.

Adapters may ingest or project facts at the boundary, but they must not decide:

- which agent/session should wake;
- whether a message is deliverable after TTL expiry;
- whether a task/assignment is complete;
- whether a delegated run is terminal;
- whether a `/new` request clears state in place;
- whether two MCP tools are safe to merge;
- whether Den product state replaces Rust runtime state.

When an adapter needs executable behavior, it submits a typed ingress/control
request to Rust or to the TS host layer that calls Rust. Runtime state changes
then come back as Rust events/projections.

## Ownership Split

### Rust Owns

- internal agent/session/message coordination;
- wake eligibility and queued-message TTL policy;
- session lifecycle and delegated session lifecycle;
- durable identity, session config, worker-run, counter, search, and binding
  persistence;
- control APIs for create/archive/cancel/checkpoint/reload/maintenance;
- typed runtime events and diagnostics projections.

### TypeScript Adapters Own

- external protocol connections;
- Den Channels WebSocket/HTTP/fallback/reconnect behavior;
- Telegram Bot API and provider-specific formatting;
- MCP client transports, discovery, JSON Schema conversion, and tool execution;
- external protocol cursors, acknowledgements, and transport diagnostics;
- projection into Den observation or channel surfaces.

### Shared Boundary

The shared boundary is typed records and commands:

- adapter registrations;
- per-agent/session/profile binding records;
- normalized channel events/projections;
- MCP binding and tool inventory reports;
- runtime ingress/control requests;
- diagnostics and observation events.

## Binding Identity

Every external surface must resolve to concrete runtime identity when available:

- `agent_id`: logical agent identity;
- `instance_id`: concrete hosted runtime instance;
- `session_id`: current executable session;
- `profile_id`: profile/toolset identity;
- `adapter_id`: Den Channels, MCP, Telegram, CLI, TUI, or future provider;
- provider-specific external IDs.

Bindings are per agent/session/profile. They are not process-global config.

Examples:

- Agent A may be bound to Den channel 100 and MCP server set `project-alpha`.
- Agent B may be bound to Den channel 100 and MCP server set `review-tools`.
- Agent A may have two channel bindings for two channels.
- `/new` archives the old session and creates a new binding target for the same
  external channel, instead of clearing the old session in place.

## Channel Adapter Boundary

Channel adapters normalize external conversation systems into Rusty Crew channel
records before any runtime behavior is requested.

Channel adapters may:

- maintain transport connections and reconnect cursors;
- translate provider messages into normalized inbound events;
- translate outbound projections into provider-specific messages;
- report presence, membership, subscriptions, and readback capability;
- cite channel/message/thread/user refs in observation payloads;
- mark themselves degraded when transport or identity resolution fails.

Channel adapters must not:

- store the only copy of Rust internal message truth;
- route messages directly to a brain without Rust wake policy;
- redeliver old external messages after cursor/TTL expiry;
- infer task completion from channel prose;
- require Den Channels schema to leak into Rust core;
- share one global active agent for all channels.

Den Channels needs an anti-corruption layer. If Den Channels changes transport
or DTO shape, only `adapter-den` should change; normalized internal channel
contracts and Rust persistence should not.

Telegram and future providers should implement the same normalized channel
contract after Den Channels stabilizes.

## MCP Adapter Boundary

MCP remains TS-side for now. It is a tool-surface adapter, not a second global
tool registry.

MCP adapters may:

- connect to configured MCP servers per profile/session/agent;
- discover tools and resources;
- convert MCP JSON Schema into Rusty Crew tool descriptors;
- feed discovered tools into the canonical registry/inventory model;
- execute selected MCP tools during a brain turn;
- report MCP lifecycle, reload, discovery, and collision diagnostics.

MCP adapters must not:

- inject tools directly into an agent outside canonical registry selection;
- use one global MCP connection set for all agents;
- store secrets in binding records;
- bypass Rust session `ToolProfile` auditability;
- mutate runtime state except through typed tool/brain actions accepted by Rust;
- let reload for one session alter another session's MCP surface.

MCP binding records should store endpoint/config references, selected server
names, transport kind, tool-profile string, discovered tool revision, status,
and diagnostics. They should not store bearer tokens or raw secret values.

## Adapter Ingress

Adapters can inject these categories:

- Den product-data updates as Den-owned references;
- normalized channel inbound messages;
- external adapter status/degraded/recovered notifications;
- tool catalog changed notifications;
- explicit control requests submitted through admin/slash surfaces.

Ingress must include enough identity and correlation to resolve:

- target agent/session/profile;
- source adapter/provider;
- external channel/thread/message/user IDs when relevant;
- cursor or idempotency key for duplicate suppression;
- received timestamp and TTL/replay policy.

If identity cannot be resolved, the adapter should degrade visibly and avoid
guessing a runtime target.

## Adapter Projection

Adapters can project these categories:

- outbound channel messages;
- channel breadcrumbs/status/evidence posts;
- Den observation `agent_activity.v1` breadcrumbs;
- adapter diagnostics;
- readback summaries.

Projection failure must not block Rust coordination. It should be recorded as
adapter degraded state and surfaced in diagnostics/observation when configured.

## Failure And Degraded Behavior

Internal agent-to-agent routing must keep working when an external adapter is
down.

Adapter failure behavior:

- transport disconnected: mark adapter degraded/disconnected;
- projection failed: drop or retry according to adapter policy, but do not
  roll back Rust state;
- ingress identity missing: reject/degrade instead of routing to a guessed
  agent;
- stale cursor or replay window exceeded: do not resurrect old messages;
- MCP discovery failed: keep prior selected tools only if policy allows and the
  binding status says stale/degraded;
- MCP reload failed: leave the previous session surface intact unless explicit
  policy says reload is atomic and should disable the surface.

Diagnostics reason codes should include:

- `adapter_disconnected`;
- `adapter_projection_failed`;
- `missing_binding`;
- `missing_canonical_identity`;
- `stale_external_cursor`;
- `expired_external_message`;
- `mcp_discovery_failed`;
- `mcp_tool_collision`;
- `mcp_reload_failed`;
- `secret_reference_missing`.

## Persistence Requirements

Later binding persistence should support:

- many bindings per agent;
- many agents per external provider;
- bindings scoped to agent/instance/session/profile;
- external provider refs without secret material;
- status and degraded reason;
- cursor/subscription/provenance metadata;
- MCP discovered tool revision and selected server set;
- audit timestamps.

This should live behind `core-persistence` typed APIs so a future PostgreSQL
move does not leak through adapters.

## Observation Requirements

Adapter activity can produce Den observation events, but observation remains
display-only.

Adapters should emit `agent_activity.v1` for:

- adapter connected/disconnected/degraded/recovered;
- channel binding established or lost;
- visible work checkpoints;
- long/risky tool calls;
- MCP reload/discovery summaries;
- admin/slash lifecycle actions.

Observation payloads should cite refs such as channel ID, message ID, session
ID, task ID, run ID, or tool revision. They must not include raw prompts, full
tool output, secrets, or executable commands.

## Implementation Order

1. Define normalized channel contract.
2. Persist channel and MCP bindings.
3. Build Den Channels anti-corruption adapter.
4. Add transport/cursor discipline and multi-agent routing.
5. Implement channel ingress/projection/readback.
6. Define and implement per-agent MCP surfaces.
7. Integrate MCP discovery through the canonical registry.
8. Add adapter diagnostics and multi-agent tests.
9. Prove Den Channels and MCP independently end to end.
10. Port Telegram once the channel contract is stable.

## Non-Goals

- Do not make Den Channels the Rusty Crew bus.
- Do not make MCP a separate global tool registry.
- Do not port Telegram before the normalized channel contract is stable.
- Do not expose old pi-crew centralized worker-pool adapter assumptions as the
  default full/prime-agent model.
- Do not store secrets in runtime binding records.
