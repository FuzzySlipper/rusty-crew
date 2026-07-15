# Review-Agent Inbox and Prompt Guidance

Use this guide for a dedicated review agent whose profile sets
`externalMessageDeliveryPolicy` to `serial_next_turn`. Crew, not the model,
owns the durable FIFO inbox, claim, expiry, reply correlation, and restart
behavior.

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
Replace only `<reviewer-agent-id>` with the stable ID returned by
`rusty_crew.list_agents` or `list_agents` on the same service.

```md
When review is needed, call `list_agents` and locate the routable review agent.
Use its stable agent ID `<reviewer-agent-id>`, never its display label, profile
ID, session ID, or Codex thread name.

Submit one self-contained asynchronous request with `send_agent_message`. Set
the recipient to `<reviewer-agent-id>`, choose a `ttlSeconds` from 1 through
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

The stable `agentId` is the sender-facing address. Crew resolves it to the
currently active session and persists that concrete recipient `sessionId` on
acceptance. Profile IDs, labels, and Codex thread names are descriptive and are
not routing keys. If the recipient session is replaced before queued work runs,
Crew rejects that work with `agent_message_recipient_session_changed` instead
of delivering it to a different session.

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
| `send_agent_message` | `toAgentId`, `body` | `correlationId`, `requireWake`, `ttlSeconds` |
| `reply_agent_message` | `messageId`, `body` | `ttlSeconds` |
| `agent_round` | `toAgentId`, `body` | `correlationId`, `timeoutMs` |

Both message tools default to a 300-second TTL and accept integer `ttlSeconds`
values from 1 through 86,400. Review requesters should set the TTL explicitly
based on expected queue delay.

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
  `agent_message_reply_original_expired`, and
  `agent_message_reply_wrong_recipient` for invalid reply attempts;
- `agent_message_recipient_session_changed` when queued work cannot safely
  follow a replaced reviewer session.

Expiry is checked before claim and before native turn start. Expired, rejected,
and failed requests are terminal; restart and queue repair do not resurrect
them. Retrying is an explicit new review request with a new durable identity.

## Operator Readback

Use the deployment-specific admin route and optional `toAgentId`/`limit` query:

- production: `GET /v1/coordination/messages`
- debug: `GET /v1/debug/coordination/messages`

The response includes each delivery, linked reply, inbox status, queued-message
ID, external-turn request ID, and terminal reason. The production and debug
routes are intentionally separate; there is no debug selector on the
production route.

The live FIFO, reply, restart, and expiry proof is recorded in
[`evidence/task-5806-serial-review-inbox-live.md`](evidence/task-5806-serial-review-inbox-live.md).
