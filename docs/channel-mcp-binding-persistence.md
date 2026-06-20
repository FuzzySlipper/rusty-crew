# Channel And MCP Binding Persistence

Status: Implemented persistence slice for task 2928

Date: 2026-06-20

Depends on:

- `multi-agent-adapter-architecture`
- `normalized-channel-adapter-contract`
- `unified-runtime-persistence-architecture`

## Purpose

Rusty Crew can host many agents in one service. Each agent may have independent
channel surfaces and MCP tool connections, even when several agents share the
same external provider channel or server process.

The Rust persistence layer stores durable binding records for those surfaces.
Adapters use the records to resolve identity and diagnostics, but the records do
not make adapters the owner of runtime routing, wake policy, task state, or
message truth.

## Channel Bindings

`channel_bindings` records one external conversation surface bound to one Rusty
Crew runtime identity scope.

Important fields:

- `binding_id`: stable Rusty Crew binding ID.
- `adapter_id`: adapter instance such as `den-channels-main`.
- `provider`: provider key such as `den_channels`, `telegram`, or `simulated`.
- `agent_id`, `instance_id`, `session_id`, `profile_id`: resolved runtime
  identity.
- `external_channel_id`, `external_thread_id`, `external_user_id`: provider
  refs kept as strings.
- `provider_subscription_id` and `cursor`: adapter transport state.
- `membership_state`, `presence_state`: observed provider state only.
- `status` and `degraded_reason`: binding health.
- `provenance_json`: safe diagnostic/source metadata.

The same external provider/channel can appear in multiple rows. That is
intentional: each agent/session/profile binding owns its own cursor,
subscription, status, and diagnostics. There is no global current channel agent.

## MCP Bindings

`mcp_bindings` records one MCP tool surface bound to one Rusty Crew runtime
identity scope.

Important fields:

- `binding_id`: stable Rusty Crew binding ID.
- `adapter_id`: MCP adapter instance, usually TS-side.
- `agent_id`, `instance_id`, `session_id`, `profile_id`: runtime identity.
- `server_names_json`: selected MCP server names.
- `endpoint_ref`: config reference, not a secret or raw credential.
- `transport`: transport label such as `stdio`, `http`, or `websocket`.
- `tool_profile_key`: profile string/tool-surface selector.
- `discovered_tool_revision`: catalog revision observed by the adapter.
- `status`, `degraded_reason`, `diagnostics_json`: health and safe debug
  state.

The tool catalog may still be mostly TS-defined. Rust persists the durable
binding and can observe catalog revision/status without storing process secrets
or becoming coupled to a particular MCP client implementation.

## Secret Boundary

Binding records store references and diagnostic summaries only.

Do store:

- config keys such as `config://mcp/alpha`;
- provider IDs;
- server names;
- cursor/subscription IDs;
- safe errors and timestamps.

Do not store:

- tokens;
- raw private keys;
- complete provider auth headers;
- unbounded transcripts;
- provider DTO blobs that contain secrets.

## Query And Scale Shape

The persistence API supports bounded queries by agent, instance, session,
profile, adapter, provider, external channel, and status. Indexes cover the hot
lookup paths needed by channel dispatch, MCP refresh, and diagnostics:

- channel by agent/provider/status;
- channel by profile/agent/status;
- channel by session/status;
- channel by provider/channel/thread;
- MCP by agent/profile/status;
- MCP by session/status;
- MCP by adapter/status.

This is the expansion point for dozens of hosted agents without merging their
channel cursors or MCP tool profiles.

## Adapter Usage Rule

Adapters should resolve a binding before requesting runtime behavior.

For channels, that means inbound messages run through binding lookup,
idempotency checks, cursor/TTL checks, and only then request Rust routing or
wake behavior.

For MCP, that means tool discovery and execution use the binding's runtime
identity and `tool_profile_key`, while endpoint secrets remain behind adapter
configuration.

Projection failures should update binding health and diagnostics. They must not
roll back accepted runtime events.

