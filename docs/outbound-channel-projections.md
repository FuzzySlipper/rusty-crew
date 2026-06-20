# Outbound Channel Projections

Status: Initial implementation for task 2934

Date: 2026-06-20

Depends on:

- `normalized-channel-adapter-contract`
- `multi-agent-channel-routing`
- `den-channels-presence-membership-subscriptions`
- `channel-ingress-rust-events-and-wakes`

## Purpose

Rusty Crew needs to project agent replies and runtime progress back to external
channels without making those channels part of internal routing correctness.
Outbound channel projection is display and communication infrastructure. If it
fails, the adapter/binding can degrade, but Rust coordination state must remain
valid.

## Projection Types

The initial helper lives at:

`ts/packages/adapter-den/src/channel-projection.ts`

It produces two normalized records:

- `NormalizedChannelOutboundMessage` for agent replies to external channel
  users or threads;
- `NormalizedChannelActivityProjection` for breadcrumbs, completion notices,
  progress summaries, and error/status notices.

The Den Channels adapter can convert these records with existing
`toDenChannelsPostMessageRequest` and `toDenChannelsActivityRequest` helpers.

## Correlation

Agent replies use channel correlation IDs when available. A correlation ID of
the form `channel:<binding_id>:...` resolves the outbound binding. This lets an
agent reply to the external user that triggered a wake without needing the brain
to know provider-specific routing details.

The projection keeps:

- binding ID;
- runtime agent/session/profile identity;
- provider/channel/thread refs;
- idempotency key;
- correlation ID;
- optional work/result refs.

## Bounds And Payload Hygiene

Projection text is bounded before it reaches the provider adapter. Long agent
replies are truncated with a marker. Activity summaries intentionally avoid
large or sensitive tool payloads. Tool events mention the tool name and phase,
not inputs, outputs, or inline artifacts.

Evidence and completion projection should use `resultRef` or `workRef` handles
instead of embedding large result bodies.

## Failure Behavior

`dispatchChannelMessageProjection` and `dispatchChannelActivityProjection` catch
sink failures and return a degraded result. They do not throw by default and do
not roll back internal Rust state.

This mirrors the adapter boundary rule: channel projection is best-effort
observability/communication, not the internal coordination bus.

## Covered Cases

`smoke:den` now covers:

- agent reply projected to a channel binding using the ingress correlation ID;
- outbound body truncation;
- completion activity projection with `resultRef`;
- provider request conversion preserving correlation/binding metadata;
- failed activity sink returning a degraded result without blocking later
  projection.
