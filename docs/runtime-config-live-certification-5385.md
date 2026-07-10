# Runtime Config Live Certification

Task: #5385  
Date: 2026-07-09

## Substrates

The certification used both installed service backends:

- SQLite debug service: `http://127.0.0.1:9348`, data directory
  `/home/system/rusty-crew-debug`
- PostgreSQL live service: `http://127.0.0.1:9347`, data directory
  `/home/system/rusty-crew`

Both services reported `health: ok`, `degraded: false`, and their expected
storage backend before the test.

## API Exercise

Disposable profiles were created through
`POST /v1/admin/control/profiles` and read through
`POST /v1/admin/control/profiles/{profile_id}/read`. The backend derived each
brain implementation id, agent id, and session id.

The revisioned profile runtime-config API then changed each profile from a
chat-completions provider to the `gpt` Responses provider and added a Den MCP
binding. Readback showed:

- brain module `openai-responses` with strategy `replay`;
- an active `config://mcp/den` binding scoped to the profile session;
- `configReloadRequired`, `runtimeRebuildRecommended`, and
  `mcpRefreshRecommended` implications.

The profile update API added:

- background review on a `*/15 * * * *` schedule;
- `/home` workdir, 3-level delegation depth, and a 3,600,000 ms resource cap;
- a 77-message history window and explicit turn limits.

`/v1/admin/diagnostics/config` reported zero diagnostics and one Rust-derived
`runtime.review.memory_skills` host job for each disposable profile.
`/v1/chat/sessions` reported the expected resource, owner, history, and timeout
defaults on both backends. An explicit
`POST /v1/admin/control/config/reload` retained those values without a service
restart.

## Replacement Session

The guarded rebuild plan correctly declared that replacement would not
preserve session identity or history, would start with an empty queue, and
would refresh the profile MCP binding. Applying the plan on both backends:

- archived the old session and created the requested replacement session;
- discarded Responses provider state;
- moved and successfully refreshed the MCP binding;
- updated profile-registry session references;
- preserved the old queue without copying expired queued messages.

## Defect Found And Fixed

The first no-restart replacement attempt found a stale-runtime authority bug.
Config reload recomputed candidate brain diagnostics even when Rust rejected a
duplicate brain registration and retained the old handle. A pi-agent handle
could therefore be described as an OpenAI Responses handle, causing provider
state cleanup to fail with `brain registration does not use provider state`.

Runtime config application now carries forward the module selection and
diagnostics belonging to an already-registered handle. Candidate metadata only
becomes active after registration or an explicit runtime rebuild succeeds.

The repaired no-restart path was exercised on SQLite:

1. A Responses profile was changed back to the chat-completions provider.
2. Mutation readback showed pi-agent as the candidate while active runtime
   diagnostics correctly remained Responses.
3. A preserve-identity rebuild cleared Responses state and switched the same
   session to pi-agent.
4. The chat context API reported provider `tester-chat`, brain `pi-agent`, and
   strategy `default`.

The replacement smoke also passed after its removed deterministic provider
fixture was updated to a valid current custom provider shape.

## Cleanup

Both disposable profiles were hard-deleted through the control API. The API
removed their profile files, runtime graph entries, old and replacement
sessions, MCP bindings, registry records, and profile-owned SQLite/PostgreSQL
rows. Both installed services remained healthy after cleanup.

