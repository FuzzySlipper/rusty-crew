# Task 5851 Profile Create Prompt Evidence

Date: 2026-07-15 PDT

## Scope

This certification verifies that the public profile-create path persists exact
DB-backed prompt text before runtime materialization and that a Crew-managed
Codex session receives the saved soul as `developerInstructions`.

## Environment

- Service: `rusty-crew-debug.service`
- API: `http://127.0.0.1:9348`
- Storage: SQLite, as required by the debug-service topology
- External runtime: `rv-live-codex-5516`
- Observed Codex CLI: `0.144.4`

The task's older reference to a Postgres-backed service on port `9348` does not
match the deployed topology. Port `9348` intentionally exercises SQLite while
the live service on port `9347` exercises Postgres. The repository Postgres
backend gate was run against the dedicated local `rusty_crew_test` database and
passed all 28 backend conformance tests, including profile registry and session
memory governance.

## Procedure And Results

1. Created disposable profile `task5851-cert-1784174328` through
   `POST /v1/admin/control/profiles` with `providerAlias: tester-chat`, an exact
   multiline `soulMarkdown`, and an exact multiline `memoryMarkdown`.
2. Read the record through
   `GET /v1/admin/profiles/registry/task5851-cert-1784174328`. Both prompt
   fields matched the submitted strings byte-for-byte, including leading
   spaces and trailing newlines.
3. Restarted `rusty-crew-debug.service` and repeated the registry read. Both
   exact comparisons still passed.
4. Created a native Codex session through `POST /v1/external-agent-sessions`.
   Creation reached `ready`, binding
   `external-binding-39acfcfedaf57c26542a2e98`, with a non-empty persisted
   profile prompt hash.
5. Asked the native session for the marker from its developer instructions.
   Native turn `019f6914-3a1e-79e2-92e8-96086be7ecad` completed and returned
   exactly `TASK_5851_SOUL_1784174328`.
6. Deleted the native thread and hard-deleted the disposable profile. Registry
   readback returned HTTP 404 after cleanup.

The live marker result complements the controller contract test that inspects
the exact `thread/start` request and asserts that the profile soul is sent as
`developerInstructions` without replacing Codex `baseInstructions`.
