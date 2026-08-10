# Telegram install diplomat administration

The Telegram install diplomat is an external adapter backed by one bot token
per Crew installation. Its routing authority is an exact, revisioned
`bot + chat + optional topic -> full session` binding stored by Rust.

The operation-specific generated-client contract for every route and readback
shape is `docs/telegram-diplomat-admin-api-v0.openapi.json`. The broader API
capability artifact remains discovery metadata and does not replace this wire
contract.

It is deliberately not profile configuration. Creating, moving, pausing, or
removing a diplomat binding does not mutate a profile, a session workspace, or
either the old or new session. There is no project allowlist, Telegram-user
allowlist, Telegram privilege plane, or profile workspace confinement in this
surface. The selected session's normal harness and tool permissions remain
authoritative.

## Provisioning flow

1. Enable the adapter and optionally select a credential id with
   `RUSTY_CREW_TELEGRAM_CREDENTIAL_ID` (default `telegram-main`).
2. `POST /v1/admin/telegram-diplomat/credential` with `{ "token": "..." }`.
   The raw token is stored through the service credential store and is never
   returned by diplomat readback or persisted in a binding.
3. Read `GET /v1/admin/telegram-diplomat` until the bot identity is present.
   Candidate chats and topics appear after the bot observes their updates.
4. Create a binding with `POST /v1/admin/telegram-diplomat/bindings`, supplying
   the installation label, exact agent/session, and selected chat/topic.

The credential endpoint also supports token rotation. Credential writes and
binding mutations reload the connector. If reload or bot identification fails,
active bindings are revisioned to `needs_rebind` so readback cannot claim an
active route that is not usable.

## Binding controls

- `GET /v1/admin/telegram-diplomat/bindings/{binding_id}`
- `POST /v1/admin/telegram-diplomat/bindings/{binding_id}/move`
- `POST /v1/admin/telegram-diplomat/bindings/{binding_id}/relabel`
- `POST /v1/admin/telegram-diplomat/bindings/{binding_id}/pause`
- `POST /v1/admin/telegram-diplomat/bindings/{binding_id}/resume`
- `POST /v1/admin/telegram-diplomat/bindings/{binding_id}/remove`
- `POST /v1/admin/telegram-diplomat/reload`

Mutations accept `expectedRevision`. A move changes only the binding's exact
agent/session target; it preserves both sessions and all profile/workspace
state.

## Readback states

The collection readback returns one of `disabled`, `unconfigured`,
`disconnected`, `unbound`, `ambiguous`, `rate_limited`, or `healthy`, together
with redacted credential metadata, Bot API identity, discovered surfaces,
durable bindings, cursor/polling/delivery/media counters, loop-budget outcomes,
and the last connector error.
