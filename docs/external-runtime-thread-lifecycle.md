# External Runtime Thread Lifecycle

Rusty Crew exposes native Codex thread history lifecycle through the external
runtime controller. Browser clients must use these Crew routes rather than call
Codex app-server directly:

- `POST /v1/external-runtimes/{runtime_id}/threads/{thread_id}/archive`
- `POST /v1/external-runtimes/{runtime_id}/threads/{thread_id}/unarchive`
- `GET /v1/external-runtimes/{runtime_id}/threads?archived=true`

The generated wire contract is
[`external-runtime-api-v0.openapi.json`](external-runtime-api-v0.openapi.json).

## Invariants

Archive is the reversible default removal action. Crew rejects it while either
the native thread is active, a Crew external turn targets the thread, or an
unresolved external interaction targets the thread.

Archiving native history also changes every associated Crew binding to
`archived`. If any binding write fails after the native archive, the controller
attempts to restore both the bindings already changed and the native thread. A
failure is reported explicitly, including compensation failures; Crew never
reports a successful operation while knowingly leaving a silent split.

Unarchive restores native history only. It does not reactivate archived Crew
bindings or resume an agent association. Binding reactivation must be a separate
explicit operator action so restoring old history cannot resurrect old work.

Archive and unarchive are idempotent. Repeating archive returns
`already_archived`; repeating unarchive returns `already_active`. A thread absent
from both native catalogs returns `external_thread_not_found`.

Codex app-server notifications for `thread/archived` and `thread/unarchived`
flow through the existing normalized `thread_lifecycle` event path. Native
payloads remain subject to the bounded raw-detail policy.

## Hard Deletion

The pinned Codex app-server protocol `0.144.1` has no supported thread
hard-delete request. Crew therefore exposes no hard-delete route and does not
remove Codex rollout files or edit its state database. Den task `#5708` records
the protocol-upgrade follow-up.
