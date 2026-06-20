# Channel Ingress Into Rust Events and Wakes

Status: Initial implementation for task 2933

Date: 2026-06-20

Depends on:

- `den-channels-transport-cursor-discipline`
- `multi-agent-channel-routing`
- `den-channels-presence-membership-subscriptions`
- `queued-message-retention-state`

## Purpose

Channel ingress must turn external channel messages into normal Rust-owned
coordination inputs. The adapter may normalize provider payloads and resolve a
binding, but it must not wake brains directly or decide work ownership outside
the Rust message path.

## Ingress Order

The implemented path is:

1. provider transport applies cursor, duplicate, and TTL checks;
2. normalized inbound message is checked for expiry again at ingress;
3. channel route resolution picks an active binding or fails closed;
4. adapter injects a typed `channel_message` external event for audit and
   correlation;
5. adapter routes an `AgentMessage` through Rust `routeAgentMessage`.

Expired, duplicate, stale-cursor, ambiguous, inactive, and missing-binding
messages do not reach Rust ingress. This avoids resurrecting old channel
messages after reconnect or restart.

## Correlation

`channel_message` external events carry:

- binding ID;
- correlation ID;
- idempotency key;
- provider/channel/thread/message references;
- author and body text;
- received and expiry timestamps.

The same correlation ID is carried on the routed `AgentMessage`. The native
bridge now accepts the optional correlation field so replies, breadcrumbs,
evidence, and diagnostics can connect the external event to the routed agent
message.

## Current Location

The adapter helper lives at:

`ts/packages/adapter-den/src/channel-ingress.ts`

The helper depends only on a small bridge-shaped interface:

- `injectExternalEvent(event)`;
- `routeAgentMessage(message)`.

This keeps the Den Channels adapter independent from the concrete native bridge
while still exercising the same Rust-owned coordination calls.

## Covered Cases

`smoke:den` now covers:

- accepted Den Channels message injects one typed external event and routes one
  correlated agent message;
- duplicate transport decision does not inject or route;
- stale cursor transport decision does not inject or route;
- expired transport decision does not inject or route.
