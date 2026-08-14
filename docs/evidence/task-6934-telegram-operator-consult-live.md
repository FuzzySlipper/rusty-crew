# Task 6934 remote Telegram operator consult certification

## Result

Passed on 2026-08-14 against the remote m5 installation's native instance A.
An ordinary non-Telegram chat wake called `request_telegram_consult` exactly
once, Telegram delivered a fresh message to the session's bound support room,
and a human reply returned through ordinary Telegram ingress to the same Crew
session. The session then completed normally and its ordinary Telegram response
was delivered.

This evidence closes Den review finding `R6934-1`. Numeric Telegram user IDs and
the full chat ID are intentionally omitted. The chat is identified only by the
masked suffix `…7489`; the bounded certification phrase and Telegram message
IDs contain no credentials or unrelated conversation content.

## Exact deployed revisions

- Rusty Crew: `7790a1c24a23bac75b5849b503c013e217c23b25`
- Rusty View: `c7d14ddadee84c383550554ff68e5f0bc8282ad4`
- Paired release: `20260814T012542Z-7790a1c24a23-c7d14ddadee8`
- Instance: remote m5 native instance A, service port 9347
- Profile/session: `ambassador` / `ambassador-session`
- Binding: active install-diplomat binding for adapter `telegram-main`, masked
  chat `…7489`, participation mode `mention_or_reply`
- Profile readback: revision 4, `telegram_diplomat` present in requested
  toolsets, bounded remote-consult guidance present

The exact Crew revision had already passed Den gate 3383 with GitHub jobs
`Verify Offline` and `Verify Postgres Backend` before deployment.

## Outbound proof from a non-Telegram wake

The certification originated through
`POST /v1/chat/sessions/ambassador-session/messages`, not Telegram:

- client message: `task-6934-remote-certification-2`
- chat event: `message_created` sequence 22254 at
  `2026-08-14T01:36:26.413Z`
- wake: `service-ambassador-session-1786671386417-1`
- reason: task 6934 remote non-Telegram consult certification after profile
  hydration

The same wake produced:

- `tool_call_started` sequence 22315 for `request_telegram_consult` at
  `2026-08-14T01:36:31.277Z`
- `tool_call_completed` sequence 22317 with `is_error: false` at
  `2026-08-14T01:36:32.310Z`
- `logical_turn_completed` sequence 22501 with one committed tool operation
  and a completed operator state

Read-only SQLite authority readback from
`telegram_operator_consults` showed one record:

- consult ID:
  `telegram-consult-64fbcd3dc42876ef7ee7b692ffe8a66d472416e263242e993585da815b4b2daf`
- exact session/profile: `ambassador-session` / `ambassador`
- exact wake: `service-ambassador-session-1786671386417-1`
- category: `other`
- status/revision: `sent` / 2
- delivery attempts: 1
- external message IDs: `["45"]`
- reason: `telegram_operator_consult_sent`
- last error: none
- requested/sent: `2026-08-14T01:36:31.275Z` /
  `2026-08-14T01:36:32.281Z`

No destination identifier was supplied by the model. Rust resolved the exact
active binding, and connector receipt `45` was a fresh outbound message rather
than a reply to an older Telegram update.

## Human reply and same-session ingress proof

The operator replied directly in Telegram with the bounded phrase
`m5 consult received` as external message `46`. The durable
`telegram_diplomat_interactions` record showed:

- interaction schema: `telegram_diplomat_interaction.v1`
- external message: 46
- sender kind: `human`
- visible sender handle: `@patchfoot`
- binding: the same active `telegram-main` / masked chat `…7489` binding
- created at: `2026-08-14T01:44:17.000Z`

Connector diagnostics after ingress showed:

- `humanMessages: 1`
- `routed: 1`
- `unbound`, `ambiguous`, `expired`, `duplicate`, `failed`, `retryPending`,
  `quarantined`, `loopTerminated`, and `rateLimited`: all 0

That ingress admitted wake `service-ambassador-session-1786671857431-2` on
`ambassador-session` at `2026-08-14T01:44:17.460Z`. The model-visible frozen
input contained the exact bounded reply phrase. The wake completed at
`2026-08-14T01:44:24.439Z`, and session execution returned to idle at
`2026-08-14T01:44:24.449Z`. The connector then delivered the ambassador's
ordinary final response as external message `47`. This final response used the
existing Telegram completion projection; it did not require another consult
tool call.

## Behavioral observation

The DeepSeek-backed ambassador followed explicit bounded guidance: it invoked
the consult tool once, did not duplicate the request, waited for the operator,
and recognized the reply on the next ordinary ingress wake. This is evidence
that the capability is usable, not evidence yet about how often the model will
choose it without an explicit certification prompt. Future real use should be
observed before adding enforcement or rate policy.

The first bounded attempt also exposed a deployment/configuration nuance. The
tool appeared in the newly applied profile inventory, but the already-running
session retained its pre-change executor and rejected that attempt without a
Telegram delivery. Restarting instance A hydrated the persisted revision-4
profile; the single rerun then passed. This did not produce a duplicate or
partial outbound consult. Operators adding this tool to an already-live profile
should currently reload the owning service/session before testing it.
