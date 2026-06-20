# Den Channels Anti-Corruption Adapter

Status: Initial typed adapter boundary for task 2929

Date: 2026-06-20

Depends on:

- `normalized-channel-adapter-contract`
- `channel-mcp-binding-persistence`

## Purpose

Den Channels can change its transport and payload shape without forcing Rusty
Crew to redesign runtime messaging. The `adapter-den` package owns that
translation boundary.

The current implementation is fixture-backed rather than a live transport. It
normalizes known old/current Den Channels message shapes into the shared channel
contract and exposes small conversion helpers for the bridge edge and outbound
Den Channels API requests.

## Boundary

Den Channels-specific DTOs live in
`ts/packages/adapter-den/src/den-channels.ts`.

Shared channel shapes live in `@rusty-crew/contracts`:

- `NormalizedChannelInboundMessage`
- `NormalizedChannelOutboundMessage`
- `NormalizedChannelActivityProjection`

Rust-owned routing, wake decisions, task state, queue retention, and durable
message truth do not move into the adapter.

## Inbound Flow

The adapter accepts Den Channels fixture shapes:

- legacy `type: "message"` / `type: "den_channel_message"` payloads;
- current `kind: "channel.message.created"` / `kind: "message.created"`
  payloads.

The adapter maps these to `channel_inbound_message.v1`, including:

- binding ID;
- adapter ID;
- runtime identity when resolved;
- provider refs;
- author;
- body/summary;
- attachments and mentions;
- received/expiry timestamps;
- cursor;
- idempotency key;
- source-shape provenance.

Only after normalization can the adapter convert a non-expired message to the
current bridge `human_message` external event payload. Expired messages convert
to bounded `raw_json` terminal inspection payloads and should not create pending
runtime delivery.

## Outbound And Activity Projection

The adapter translates normalized outbound channel messages to Den Channels post
requests with channel/thread refs, idempotency key, delivery policy, visibility,
correlation, and work/result references.

It also translates normalized activity projections to Den Channels activity
requests. Projection failures should degrade binding/adapter status and record
diagnostics; they must not roll back Rust runtime state.

## Future Transport Work

Task 2930 should plug WebSocket/HTTP/reconnect/cursor behavior into this module
instead of leaking Den Channels transport details into Rust or the generic
channel contract.

