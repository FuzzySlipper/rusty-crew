# Task 5806 Serial Review Inbox Live Evidence

Date: 2026-07-15 UTC  
Service: debug-only `http://127.0.0.1:9348`  
Storage: debug SQLite service  
External runtime: attached Codex app-server `rv-live-codex-5516`

## Topology

- Reviewer profile: `reviewer-cert-5806`
- Reviewer agent: `external-agent-9ce965fcb138aa92f87cd58b`
- Reviewer session: `external-session-9ce965fcb138aa92f87cd58b`
- Reviewer binding policy: `serial_next_turn`
- Sender agents:
  - `external-agent-4482a23402438302386cb5ff`
  - `external-agent-40e714c50012e36c2d55d417`
  - `external-agent-fc8d4873ac1690460edf279e`

The reviewer profile used profile-scoped developer instructions that require one
request per turn and one `rusty_crew.reply_agent_message` call using the routed
message ID.

## FIFO And Reply Linkage

Sender A submitted a request that ran `sleep 20` before review. While that turn
was active, senders B and C submitted separate requests. The debug inbox API
reported, in creation order:

1. A: `in_progress`, with a direct `agent-message:*` external turn.
2. B: `queued`, with no external turn yet.
3. C: `queued`, with no external turn yet.

After A completed, B and C each received their own
`external-follow-up:agent-message-queue:*` turn in FIFO order. The final inbox
reported all three as `replied`. Each reply record had a unique reply message ID
and `replyToMessageId` exactly equal to its original request message ID. The
reviewer supplied only the request message ID and reply body; Rust recovered the
sender session and correlation metadata.

## Restart And Expiry

Sender A then submitted review D, which ran `sleep 30`. Sender B submitted review
E behind it with a one-second TTL. The service was restarted after E had entered
the durable queue and expired.

After restart:

- D resumed its existing external turn and produced one linked reply.
- E reported `expired`.
- E never acquired an external turn ID.
- E never produced a reply.

This proves the debug service does not resurrect an expired serial review after
restart or after the preceding review completes.

## Deterministic Coverage

Focused Rust coverage additionally verifies:

- SQLite FIFO promotion and one-at-a-time external turns;
- atomic queue-to-turn promotion and idempotent replay;
- completed-without-reply remains blocked as `awaiting_reply`;
- duplicate reply calls are idempotent while conflicting second replies fail;
- explicit no-reply and terminal failure behavior;
- expiry before promotion;
- recipient session replacement cancels old pending work instead of delivering
  it to the replacement session;
- PostgreSQL queue-to-turn atomicity and idempotency through the backend
  conformance gate.

