# Multi-Agent Channel Routing

Status: Initial routing resolver for task 2931

Date: 2026-06-20

Depends on:

- `normalized-channel-adapter-contract`
- `channel-mcp-binding-persistence`
- `den-channels-anti-corruption-adapter`

## Purpose

Rusty Crew can host multiple agents that share a provider and even the same
external channel. Channel ingress must resolve a durable binding before it can
route work into Rust.

The adapter may decide which binding an inbound channel message belongs to, but
it must not wake arbitrary brains. Resolved messages become explicit
`AgentMessage` route requests and go through Rust-owned coordination via
`route_agent_message`.

## Resolver Location

The initial resolver lives at:

`ts/packages/adapter-den/src/channel-routing.ts`

It operates on shared contract types:

- `NormalizedChannelInboundMessage`
- `ChannelBindingRecord`

The shared `ChannelBindingRecord` mirrors the Rust persistence shape closely
enough for adapter-side query/readback surfaces, without exposing SQLite or
adapter internals.

## Routing Order

The resolver filters active bindings by:

1. provider;
2. external channel;
3. compatible thread, when thread IDs are present.

It then tries to disambiguate in this order:

1. exact normalized `bindingId`;
2. explicit mention of an agent ID or configured mention alias;
3. normalized runtime `agentId`;
4. single remaining binding.

If more than one binding remains, the route is `ambiguous`. If only inactive
bindings match the surface, the route is `inactive_binding`. If no binding
matches the surface, the route is `no_binding`.

Ambiguous routes fail closed. The adapter should project a diagnostic or ask for
operator/admin intervention rather than guessing.

## Route Request

A successful resolution produces:

- source channel author as a synthetic `from` agent ID;
- target Rusty Crew agent ID;
- body text;
- binding ID;
- optional session ID;
- correlation ID derived from binding and idempotency key.

The current native bridge `routeAgentMessage` accepts `from`, `to`, and `body`
only. The resolver still computes a correlation ID so the bridge can be widened
later without changing channel resolution semantics.

## Covered Cases

`smoke:den` now covers:

- two active agents sharing one Den Channels provider/channel/thread;
- mention-based route to the intended agent;
- exact binding ID route to the intended agent;
- ambiguous shared-channel route without mention/runtime binding;
- inactive binding failure;
- conversion of a successful route to current bridge arguments.

