# Normalized Channel Adapter Contract

Status: Design contract for task 2927

Date: 2026-06-20

Depends on: `multi-agent-adapter-architecture`

## Purpose

Rusty Crew needs a channel contract that Den Channels, Telegram, and future
providers can implement without importing each other's DTOs or transport
details. The contract must support many agents in one service, multiple channel
bindings per agent, and independent channel activity across profiles/sessions.

Channel adapters are protocol translators. They are not the Rusty Crew message
bus and do not own task state, routing policy, wake policy, or durable internal
message truth.

## Canonical IDs

Every normalized channel record should carry stable IDs:

- `adapter_id`: Rusty Crew adapter instance, for example `den-channels-main`;
- `provider`: `den_channels`, `telegram`, `simulated`, or future provider;
- `binding_id`: Rusty Crew binding record for this external surface;
- `agent_id`, `instance_id`, `session_id`, `profile_id` when resolved;
- `external_channel_id`;
- `external_thread_id` when the provider has threads;
- `external_message_id` when the provider has message IDs;
- `external_user_id` for the external author/user/member;
- `correlation_id` for end-to-end command/projection/reply correlation;
- `cursor` or provider sequence when available.

Provider-specific IDs stay strings. Adapters should not coerce Den channel IDs,
Telegram chat IDs, or future provider IDs into a shared numeric type.

## Binding Scope

Channel bindings are per external surface and runtime identity. A provider
channel may have multiple Rusty Crew bindings if multiple agents participate in
the same channel.

Binding scope fields:

- agent;
- runtime instance;
- session;
- profile;
- provider;
- external channel/thread/user refs;
- status and degraded reason;
- cursor/subscription/provenance metadata.

There is no global "current channel agent." Routing must resolve a binding
before it can request any runtime behavior.

## Normalized Inbound Message

Inbound provider messages normalize to:

- `kind: "channel_inbound_message.v1"`;
- `binding_id`;
- canonical runtime identity fields when resolved;
- provider and external refs;
- author identity and display label;
- text body or structured body summary;
- attachments as refs/metadata, not giant inline blobs;
- mentions parsed into external refs and optional target agents;
- `received_at`;
- `ttl_ms`;
- `expires_at`;
- `cursor`;
- `idempotency_key`;
- `visibility`: `conversation`, `task`, `debug`, or `system`;
- `provenance`: provider-specific metadata safe for diagnostics.

Ingress must run duplicate and expiry checks before asking Rust to route or
wake anything. If an inbound item is expired, it may be stored as terminal
inspection state but must not become a pending message.

## Normalized Outbound Message

Outbound channel messages normalize to:

- `kind: "channel_outbound_message.v1"`;
- `binding_id`;
- runtime identity fields;
- provider/channel/thread target refs;
- body text or structured content blocks;
- optional reply target external message ID;
- `correlation_id`;
- `idempotency_key`;
- `visibility`;
- `delivery_policy`: `best_effort`, `must_ack`, or `dry_run`;
- safe `result_ref` or `work_ref` handles when this is evidence/progress.

Adapters convert this shape into provider-specific payloads. Projection failure
marks adapter/binding degraded and records diagnostics; it does not roll back
Rust runtime state.

## Breadcrumb And Status Projection

Channel breadcrumbs are display projections, not runtime facts.

Normalized breadcrumb/status shape:

- `kind: "channel_activity_projection.v1"`;
- `binding_id`;
- runtime identity fields;
- provider/channel/thread refs;
- `event_type`: stable display event such as `work_checkpoint`,
  `session_archived`, `mcp_reloaded`, `adapter_degraded`;
- short `summary`;
- `severity`: `info`, `success`, `warning`, `error`;
- `work_ref` and `result_ref`;
- `created_at`;
- optional Den observation payload ref.

For Den Web breadcrumbs, prefer emitting `agent_activity.v1` to observation and
linking channel refs from `work_ref`/`result_ref`.

## Presence And Membership

Presence and membership are separate from executable runtime state.

Membership record:

- `kind: "channel_membership.v1"`;
- `binding_id`;
- provider/channel/thread refs;
- member external user ID and display label;
- optional mapped agent/profile identity;
- role/capability labels from provider;
- status: `joined`, `left`, `invited`, `unknown`;
- `observed_at`;
- source cursor/provenance.

Presence record:

- `kind: "channel_presence.v1"`;
- `binding_id`;
- provider/channel/thread refs;
- subject external user ID or mapped agent/session ID;
- status: `online`, `idle`, `offline`, `unknown`;
- `observed_at`;
- optional expiry.

Adapters may use these to improve routing and diagnostics. They must not create
or archive sessions solely from presence changes.

## Subscriptions And Cursors

Subscriptions are provider transport state, not Rust event subscriptions.

Subscription record:

- `kind: "channel_subscription.v1"`;
- `binding_id`;
- provider/channel/thread refs;
- transport kind: `websocket`, `http_poll`, `webhook`, `simulation`;
- provider subscription ID if any;
- cursor/checkpoint;
- status: `active`, `degraded`, `disconnected`, `paused`, `archived`;
- last connected/seen/error timestamps;
- degraded reason.

Reconnect rules:

- resume from the last committed cursor only after idempotency checks;
- never replay items older than the binding replay window;
- never redeliver normalized inbound messages whose TTL expired;
- if cursor recovery is ambiguous, degrade and request operator/admin
  intervention rather than guessing.

## Readback

Readback lets an agent or operator inspect conversation context without making
channel history authoritative runtime state.

Readback request:

- `kind: "channel_readback_request.v1"`;
- `binding_id`;
- provider/channel/thread refs;
- optional `before_external_message_id` or cursor;
- limit;
- visibility filter;
- requester runtime identity;
- reason code.

Readback response:

- `kind: "channel_readback_response.v1"`;
- provider/channel/thread refs;
- normalized message summaries;
- cursor boundaries;
- truncation flag;
- provenance;
- errors/degraded reason when partial.

Readback can be exposed as an agent tool or adapter service later. It should
return bounded summaries or refs, not unbounded transcripts by default.

## Ingress To Rust

The normalized channel layer may request Rust behavior through explicit ingress
requests only after binding resolution, duplicate checks, and TTL checks.

Allowed requests:

- route a human/user message to a resolved session/agent;
- inject an external adapter-status event;
- notify tool catalog/channel binding changed;
- request a slash-command/control-plane operation.

The adapter must include:

- binding ID;
- target runtime identity;
- source external refs;
- idempotency key;
- received timestamp;
- TTL/expiry decision;
- safe body or summary.

Rust decides whether this creates a wake.

## Projection From Rust

Runtime events can be projected to channels through normalized outbound or
breadcrumb shapes.

Projection sources:

- `SessionCreated` / `SessionArchived`;
- `AgentMessageRouted`;
- `DelegationLifecycleObserved`;
- `BrainWakeRequested`;
- `BrainEventObserved` for coarse visible tool/model activity;
- `CompletionPacketDelivered`;
- admin/slash command outcomes.

Projection policy must be per binding. One runtime event may project to zero,
one, or many channel bindings depending on agent/session/profile/channel
configuration.

## Conformance Harness

Every channel adapter should pass a shared conformance harness with fake
transport fixtures.

Minimum cases:

1. Normalizes inbound human text with stable IDs, cursor, TTL, and idempotency.
2. Rejects or marks expired inbound messages without requesting Rust routing.
3. Deduplicates repeated provider messages by idempotency key.
4. Resolves two agents in the same external channel to different bindings.
5. Converts outbound message to provider payload without mutating runtime state.
6. Reports projection failure as degraded adapter/binding diagnostics.
7. Emits presence and membership records without creating sessions.
8. Resumes from cursor without replaying stale messages.
9. Produces bounded readback response with cursor boundaries.
10. Leaves provider-specific DTOs outside the normalized contract.

Den Channels, Telegram, and simulated providers should all run the same harness.

## Non-Goals

- Channel adapters do not own Den task state.
- Channel adapters do not own Rust agent routing policy.
- Channel adapters do not store the authoritative internal message history.
- Channel adapters do not infer completion from prose.
- Channel adapters do not wake brains directly.
- Channel adapters do not expose provider secrets in normalized records.
