# Task 5807 Review Guidance Live Evidence

Date: 2026-07-14 PDT (2026-07-15 UTC)

Service: `rusty-crew-debug.service` at `http://127.0.0.1:9348`

This proof exercised the copy-ready requester and reviewer behavior from
`docs/review-agent-inbox-and-prompt-guidance.md` with two real Crew-managed
Codex app-server sessions.

## Identities

- requester agent:
  `external-agent-4482a23402438302386cb5ff`
- serial reviewer agent:
  `external-agent-9ce965fcb138aa92f87cd58b`
- reviewer profile/label: `reviewer-cert-5806` / `Serial Review Cert 5806`
- correlation ID: `guidance-5807-1784077051`

The requester first called `rusty_crew.list_agents`, selected the reviewer by
stable agent ID, and called `rusty_crew.send_agent_message` once with an
explicit 600-second TTL. It did not call `agent_round` or address the reviewer
by label, profile, binding, session, or thread ID.

## Result

The serial inbox accepted the request at `2026-07-15T00:58:01.574Z`, started
one external reviewer turn, and reached terminal status `replied`. The reviewer
called `rusty_crew.reply_agent_message` once; Crew linked that reply to the
original message and routed it back to the requester.

The reviewer outcome was `BLOCKED`, correctly explaining that the requested
guide and smoke were still uncommitted and therefore absent from the pinned
commit `94e485bcbcde7c46f4e02231c89519a4ce2b8779`. It requested a committed exact
SHA instead of silently reviewing the dirty working-tree copy. This is the
expected outcome for the deliberately premature first request and proves that
the guidance supports both correlated delivery and exact-artifact review
discipline.

The inbox readback used the debug-only route
`GET /v1/debug/coordination/messages`; no production recipient or debug selector
was used.
