# External Runtime Thread Lifecycle

Rusty Crew exposes native Codex thread history lifecycle through the external
runtime controller. Browser clients must use these Crew routes rather than call
Codex app-server directly:

- `POST /v1/external-runtimes/{runtime_id}/threads/{thread_id}/archive`
- `POST /v1/external-runtimes/{runtime_id}/threads/{thread_id}/delete`
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

Codex app-server notifications for `thread/archived`, `thread/deleted`, and
`thread/unarchived` flow through the existing normalized `thread_lifecycle`
event path. Native payloads remain subject to the bounded raw-detail policy.

## Hard Deletion

The pinned Codex app-server protocol `0.144.1` includes the typed
`thread/delete` request and `thread/deleted` notification. Hard deletion is a
separate irreversible operator action; archive remains the normal removal
path.

Before deletion, Crew enumerates the root thread and its known spawned
descendants across active and archived native catalogs. It rejects the entire
operation if any scoped thread has an active native turn, active Crew turn, or
unresolved interaction. Every associated Crew binding is durably transitioned
to `archived` before the native request. A binding failure leaves native history
untouched, while a native request failure restores the changed bindings and
reports any compensation failure explicitly.

The native operation deletes the requested thread and spawned descendants.
Crew maps every `thread/deleted` notification through the bounded
`thread_lifecycle` event path. A repeated request is idempotent and returns
`already_deleted` when the root was absent from both native catalogs before the
request.

Crew never implements deletion by removing Codex rollout files or editing the
Codex state database directly.

## Live Certification

On 2026-07-12, the SQLite debug service at `http://127.0.0.1:9348` deleted the
persisted archived certification thread
`019f5652-1c67-7dc0-95c3-976ca3c5052d` through runtime
`rv-live-codex-5516`:

- the archived catalog contained exactly one matching thread before deletion;
- the first request returned `applied` with `nativeDeleted: true`;
- active and archived catalogs contained no matching thread afterward;
- associated binding `external-binding-204a0e4ec542e44d4c65b8cf` remained
  `archived`;
- a repeated request returned `already_deleted`; and
- normalized event sequence `4341` recorded `thread/deleted` as
  `thread_lifecycle` for the same native thread ID.
