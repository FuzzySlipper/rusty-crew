# Queued Message Retention State

Status: Implementation contract for task 2875

Date: 2026-06-20

## Purpose

Queued messages are dangerous when they can outlive the context that made them
fresh. Rusty Crew therefore treats persisted queue rows as recovery state with
explicit TTL and terminal states, not as an unbounded delivery backlog.

## Stored State

Each queued message records:

- message id;
- owner session and owner agent;
- original `AgentMessage`;
- optional source event sequence;
- enqueue time;
- expiry time;
- body-controlled TTL in milliseconds;
- delivery attempts;
- state: pending, delivered, expired, discarded, or cancelled;
- terminal timestamp and reason.

## Hydration Rule

Only `pending` messages whose `expires_at` is still in the future are eligible
for future delivery. Expired messages remain queryable but are terminal and must
not be redelivered.

Startup and future queue hydration paths must run expiry before exposing pending
messages to a wake.

## Search And Inspection

Queued messages are indexed in runtime search so operators or agents can find
expired/discarded work. Search visibility is not delivery eligibility.

A future expired-message pull tool should query terminal queue rows, not move
them back to pending.

## Current Integration

The persistence layer now owns the safe recovery model and expiry query path.
The body/scheduler loop does not yet use this as an active delivery queue.
Future integration must preserve the same TTL-first, terminal-state behavior.
