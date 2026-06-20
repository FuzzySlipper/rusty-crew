# Den Channels Presence, Membership, and Subscriptions

Status: Initial tracker for task 2932

Date: 2026-06-20

Depends on:

- `normalized-channel-adapter-contract`
- `channel-mcp-binding-persistence`
- `multi-agent-channel-routing`

## Purpose

Rusty Crew may host many agents on the same service process, with each agent
using its own channel binding, external channel identity, and MCP profile. The
Den Channels adapter needs normalized membership, presence, and subscription
state so operator views can distinguish channel availability from internal
runtime health.

External presence is only observation. It must not become coordination truth for
session scheduling, wake routing, ownership, or work claiming. Rust runtime state
and persisted bindings remain authoritative for those decisions.

## Tracker Location

The initial adapter-side tracker lives at:

`ts/packages/adapter-den/src/channel-presence.ts`

It emits shared contract records:

- `ChannelMembershipRecord`
- `ChannelPresenceRecord`
- `ChannelSubscriptionRecord`

Membership records answer whether the external user/binding appears joined,
left, invited, or unknown. Presence records answer whether the external surface
currently appears online, idle, offline, or unknown. Subscription records answer
whether the adapter is actively listening, degraded, disconnected, paused, or
archived.

## Rust Event Subscription Path

`ChannelBindingActivityTracker.subscribeRustEvents` accepts a small
`RustEventSubscriptionClient` interface that matches the production Rust event
subscription shape:

- subscribe by event kinds;
- optionally scope by session, agent, or adapter;
- retain the returned Rust subscription handle;
- unsubscribe using that handle.

This keeps provider transports and Rust runtime event subscriptions separate.
Channel adapters can observe Rust events when they need to project activity
without making channel presence the source of wake or routing decisions.

## Diagnostics

The tracker exposes per-binding diagnostics with:

- membership status;
- presence status;
- subscription status;
- degraded reason, when present;
- stale presence flag, based on `expiresAt`.

Diagnostics are intentionally readback-oriented. A stale or missing external
presence should explain why a channel projection may be quiet, but should not
archive a session, suppress a wake, or move work on its own.

## Covered Cases

`smoke:den` now covers:

- joined membership for a channel binding;
- idle presence with expiry and stale detection;
- Rust event subscription creation using the binding's session and agent;
- degraded subscription diagnostics;
- unsubscribe/archive behavior through the Rust subscription handle.
