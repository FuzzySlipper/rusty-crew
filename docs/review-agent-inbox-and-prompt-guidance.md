# Review-Agent Inbox and Prompt Guidance

Use this guide for a dedicated review agent whose profile sets
`externalMessageDeliveryPolicy` to `serial_next_turn`. Crew, not the model,
owns the durable FIFO inbox, claim, expiry, reply correlation, and restart
behavior.

The profile setting is persisted in the profile registry and profile runtime
config. It is used when a new managed external binding is created. Changing the
profile setting does not mutate an existing binding in place; the binding must
be explicitly rebuilt or replaced before its concrete delivery policy changes.

## Reviewer Profile Prompt

Copy this block into the review profile's soul/developer guidance. Add the
project's review procedure before or after it as needed.

```md
You are a serial review agent. Treat each Rusty Crew routed-message envelope as
one independent review job and process exactly that one request in the current
turn. Follow the request envelope and this profile's review procedure. Do not
combine, poll for, or replay later queued requests; Rusty Crew will deliver the
next unexpired request in a separate turn.

Report one clear outcome: approved, changes requested, blocked, or review
failed because a required tool/runtime was unavailable. Before ending the turn,
call `rusty_crew.reply_agent_message` exactly once with the envelope's
`message_id` as `messageId` and the complete outcome as `body`. Never guess or
provide a recipient, session ID, or correlation ID. Never claim the reply was
sent unless the tool returns an accepted receipt. If review work cannot be
completed, send the blocked or failure outcome through the same reply tool.

Queued work is Crew-owned. Expired and failed requests are terminal and are not
silently retried. Do not manually start another queued review in this turn.
```

## Review Requester Prompt

Copy this block into guidance for an agent that submits asynchronous reviews.
Replace only `<reviewer-route>` with a curated route such as `@reviewer`
returned by `rusty_crew.list_agents` or `list_agents` on the same service.

```md
When review is needed, call `list_agents` and locate the routable switchboard
route. Use its explicit address `<reviewer-route>`, never its display label,
profile ID, session ID, Codex thread name, or a guessed alias.

Submit one self-contained asynchronous request with `send_agent_message`. Set
the recipient to `<reviewer-route>`, choose a `ttlSeconds` from 1 through
86400 that covers the expected queue delay, and put the repository/path, exact
commit or artifact identity, review scope, acceptance criteria, and relevant
test evidence in `body`. Do not use `agent_round` for queued review work.

Choose a unique `correlationId` and retain it with the task or review request.
The send tool reports an accepted, queued, or routed summary; it does not return
durable message or delivery IDs to the requester. An accepted or queued summary
is not a completed review. Do not resend with a new correlation merely because
the request remains queued. Consume the correlated reply when it arrives, or
use operator readback with that correlation when diagnosing delivery, expiry,
or runtime failure. Production and debug services have separate agent
directories; never send a recipient ID discovered on one service through the
other service.
```

## Identity and Delivery Policy

The preferred sender-facing address is a DB-backed `@route`. Crew resolves it
server-side to the exact session or exact managed binding revision recorded by
the route and persists that route revision and concrete destination on
acceptance. Raw agent IDs remain available as diagnostics and explicit direct
addresses. Profile IDs, labels, and Codex thread names are descriptive and are
not routing keys. A replaced session or binding makes the route unroutable;
Crew never silently follows it to a newer destination.

`immediate_steer` delivers a new message into an already active Codex turn. It
is useful for ordinary collaboration where the new message belongs to the same
work. `serial_next_turn` never steers later requests into an active review. It
persists them FIFO and promotes one request only after the prior turn has a
durable reply, an explicit no-reply outcome, or a terminal failure. Dedicated
review agents should use `serial_next_turn`.

This inbox is a Crew coordination facility. It is not the Codex CLI's local
interactive input queue, and the model should not manage it itself.

## Tool Contracts

Managed Codex app-server sessions receive namespaced dynamic tools:

| Tool | Required arguments | Optional arguments |
| --- | --- | --- |
| `rusty_crew.list_agents` | none | none |
| `rusty_crew.send_agent_message` | `recipient`, `body` | `correlationId`, `ttlSeconds` |
| `rusty_crew.reply_agent_message` | `messageId`, `body` | `ttlSeconds` |
| `rusty_crew.agent_round` | `recipient`, `body` | `correlationId`, `timeoutMs` |

Built-in Crew brains receive the same capabilities without the namespace:

| Tool | Required arguments | Optional arguments |
| --- | --- | --- |
| `list_agents` | none | none |
| `send_agent_message` | `toAddress`, `body` | `correlationId`, `requireWake`, `ttlSeconds` |
| `reply_agent_message` | `messageId`, `body` | `ttlSeconds` |
| `agent_round` | `toAddress`, `body` | `correlationId`, `timeoutMs` |

Both message tools default to a 300-second TTL and accept integer `ttlSeconds`
values from 1 through 86,400. Review requesters should set the TTL explicitly
based on expected queue delay. The request TTL bounds delivery, claim, and turn
start; it is not a deadline for completing the review or sending the one
service-authored reply after the request was accepted. A reply has its own TTL.

The reply tool deliberately accepts no recipient or correlation field. Rust
loads the original accepted delivery, verifies the replying agent and session,
reverses the route, preserves correlation, and links the two delivery records.
Duplicate execution of the same tool call is idempotent; a distinct second
reply is rejected with `agent_message_reply_already_exists`.

## Status and Failure Handling

Inbox status is one of `queued`, `in_progress`, `awaiting_reply`, `replied`,
`no_reply`, `failed`, `expired`, or `rejected`. Useful stable reason codes
include:

- `agent_message_ttl_out_of_bounds` and `agent_message_body_size_invalid` for
  invalid requests;
- `agent_message_serial_inbox_full` when the bounded inbox is full;
- `agent_message_expired` for work whose TTL elapsed;
- `agent_message_reply_original_not_found`,
  `agent_message_reply_original_not_accepted`, and
  `agent_message_reply_wrong_recipient` for invalid reply attempts;
- `agent_message_recipient_session_changed` when queued work cannot safely
  follow a replaced reviewer session.

Expiry is checked before claim and before native turn start. Expired, rejected,
and failed requests are terminal; restart and queue repair do not resurrect
them. An accepted request's reply path instead remains pinned to its stored
sender agent/session identity. It needs no sender switchboard route and fails
closed if that exact sender session is archived or replaced. Retrying is an
explicit new review request with a new durable identity.

## Operator Readback

Use the deployment-specific admin route and optional `toAgentId`/`limit` query:

- production: `GET /v1/coordination/messages`
- debug: `GET /v1/debug/coordination/messages`

Inspect curated route resolution separately from raw inbox records:

- production: `GET /v1/coordination/routes`
- debug: `GET /v1/debug/coordination/routes`

The response includes each delivery, linked reply, inbox status, queued-message
ID, external-turn request ID, and terminal reason. The production and debug
routes are intentionally separate; there is no debug selector on the
production route.

The live FIFO, reply, restart, and expiry proof is recorded in
[`evidence/task-5806-serial-review-inbox-live.md`](evidence/task-5806-serial-review-inbox-live.md).
