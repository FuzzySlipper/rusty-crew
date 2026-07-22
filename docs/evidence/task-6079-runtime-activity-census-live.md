# Task 6079 Runtime Activity Census Live Certificate

Date: 2026-07-22

Target: `rusty-crew-debug.service` at `http://127.0.0.1:9348`, using the
deployment's isolated SQLite database. The live service on port `9347` was not
restarted or used.

## Chat Completions And Process Topology

Profile `tester` used provider alias `tester-chat` (`deepseek-flash`) and the
real `terminal` tool to run `sleep 15`. While the subprocess was alive,
`GET /v1/admin/diagnostics/activities` returned service instance
`service-698611-1` and this active tree for wake
`service-tester-session-1784717069444-2`:

```text
dispatch (type_script_host, preparing)
  wake (rust_brain, awaiting_host_tools)
    provider_request (rust_brain, awaiting_host_tools)
      tool_call terminal (type_script_host, awaiting_host)
        subprocess pid 704424 (type_script_host, running)
```

The census had zero findings and zero untracked processes. It explicitly
reported `automaticCancellationEnabled: false`. The live chat stream then
contained `tool_call_started`, `tool_call_completed`,
`assistant_message_completed`, and `assistant_turn_finished` for the same wake.

The preceding 12-second probe also left five terminal SQLite rows with the
same topology and `completed` status, proving durable lifecycle closeout rather
than only an in-memory projection.

## OpenAI Responses

A temporary profile used direct OpenAI OAuth provider alias `gpt-5.6-luna` and
the production `openai-responses` brain. During wake
`service-task-6079-responses-1784717191-session-1784717191703-4`, the census
reported an active `dispatch -> wake -> provider_request` tree with Rust brain
ownership and exact temporary session identity. The live event stream then
reported a successful tool call and `assistant_message_completed` with summary
`responses replay wake completed`. The temporary profile was deleted through
the public profile control API.

## Managed Codex

The public `POST /v1/external-agent-sessions` route created a fresh managed
Codex app-server session against ready runtime `rv-live-codex-5516`, observed
CLI version `0.144.4`. After operator message delivery, the census reported:

```text
external:agent-message:task-6079-codex-delivery-1784717597-message
session: external-session-f6296a2984189c85a103c4f8
owner: external_runtime
phase: accepted
elapsed: 71 ms
findings: none
```

The certification thread was archived and hard-deleted through Crew's typed
external-runtime lifecycle API after the probe; `nativeDeleted` was true.

Two older active/waiting Codex turns already present in the debug database were
also projected with stable session identity. After omitted census options were
normalized to Rust defaults, both correctly produced `stalled` findings from
their old progress timestamps. Stalls remained diagnostic only.

## Failure And Restart Reconciliation

A deliberately invalid existing OpenAI Responses provider configuration
returned `provider_transport_error`. The initial probe exposed an overlong
failure-summary edge case that left its dispatch record active. The dispatch
terminal metadata was changed to a fixed bounded summary, and the same live
failure was repeated after restart. Its census then contained no active rows
for the wake and three abnormal terminal rows:

- dispatch: `failed`, `provider_transport_error`, `wake dispatch failed`;
- wake: `failed`, `provider_transport_error`;
- provider request: `failed`, `provider_transport_error`.

Restart changed the service instance identity and converted the earlier
unfinished row to `interrupted` with the stable `restart_interrupted` finding.
No activity was resurrected.

## Backend And Gate Evidence

- SQLite lifecycle, revision, and restart tests passed in the normal workspace
  suite.
- `postgres_runtime_activity_lifecycle_matches_sqlite_contract` passed against
  the configured real PostgreSQL service.
- `npm run verify:offline` passed after protocol, bridge, OpenAPI, native
  surface, storage ownership, and validation artifacts were regenerated.
- Native bridge topology, local process lifecycle, browser manager lifecycle,
  admin diagnostics, and runtime diagnostics focused checks passed.

No prompt, tool argument/result, credential, provider payload, authorization
header, browser content, or full command line was persisted in activity rows or
included in this certificate.
