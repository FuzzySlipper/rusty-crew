# Den Channels Transport And Cursor Discipline

Status: Initial transport control slice for task 2930

Date: 2026-06-20

Depends on:

- `den-channels-anti-corruption-adapter`
- `channel-mcp-binding-persistence`
- `queued-message-retention-state`

## Purpose

Den Channels connectivity should be replaceable and deterministic. WebSocket,
HTTP polling, fallback transport, and simulation all sit behind the same
adapter-side transport controller.

The current implementation does not hardcode live Den Channels URLs or auth. It
adds an injectable control layer in
`ts/packages/adapter-den/src/den-channel-transport.ts` so live WebSocket/HTTP
clients can plug in later without leaking transport details into Rust-owned
messaging or brain routing.

## Transport Boundary

`DenChannelsTransport` owns:

- open/close lifecycle;
- send operation for normalized outbound channel requests;
- transport kind: `websocket`, `http_poll`, or `simulation`;
- transport-local failures.

`DenChannelsTransportController` owns:

- transport selection and failover;
- bounded retry attempts and deterministic backoff records;
- cursor resume/advance;
- duplicate suppression;
- expiry and stale-cursor rejection;
- connection status diagnostics.

The controller does not own runtime routing, wake policy, task state, queue
delivery, or durable message truth.

## Cursor Rule

On connect/reconnect, the controller reads both:

- the locally persisted cursor for the binding;
- the optional Den subscription cursor.

It resumes from the greatest cursor. This preserves the pi-channels lesson that
a subscription-owned cursor must beat a stale local cursor on restart.

Accepted inbound messages advance the local cursor. Stale cursors are dropped
and counted as diagnostics.

## Duplicate And TTL Rule

Inbound provider payloads are normalized before transport acceptance.

The controller rejects:

- expired normalized messages;
- repeated idempotency keys;
- cursors older than or equal to the last accepted cursor.

Expired messages are not pending runtime work. This is intentionally aligned
with Rust-owned queue TTL behavior so reconnects do not resurrect old channel
messages.

## Simulation

`createSimulatedDenChannelsTransport` provides deterministic tests for:

- open cursor values;
- open failures and fallback;
- outbound sends;
- closed-transport failures.

`smoke:den` currently proves:

- first transport failure falls through to fallback;
- subscription cursor `10` beats stale local cursor `2`;
- accepted inbound cursor advances to `11`;
- duplicate, stale-cursor, and expired messages are dropped;
- outbound projection sends through the active fallback transport.

## Future Live Transports

Live WebSocket and HTTP-poll transports should implement the same
`DenChannelsTransport` interface. They should not directly inject runtime
events. They should emit or pass provider payloads through the anti-corruption
normalizer and controller gates first.

