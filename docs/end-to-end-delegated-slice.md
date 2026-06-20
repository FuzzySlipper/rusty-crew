# End-to-End Delegated Worker Slice

Date: 2026-06-19

This smoke proves the current minimal planner-to-worker flow:

1. A persistent full-agent session receives an operator message.
2. The planner brain wakes through the TypeScript brain island backed by local
   den-router.
3. The planner returns a `request_delegation` action.
4. Rust validates the action, persists the worker-run request, creates a
   delegated worker session internally, routes the delegation prompt, and emits
   `brain_wake_requested`.
5. The delegated worker brain wakes through the same local den-router-backed
   brain island.
6. The worker returns a `deliver_completion` action.
7. Rust validates, routes, persists, and exposes the completion packet for Den
   observability projection.

## Command

den-router must be reachable locally. The default URL is
`http://127.0.0.1:18082`; no external API key is required because the smoke uses
the den-router provider convention `apiKey: "den-router"`.

```sh
npm run build:native
npm run smoke:delegated-slice
```

Optional overrides:

```sh
DEN_ROUTER_URL=http://127.0.0.1:18082 \
RUSTY_CREW_DEN_ROUTER_MODEL=deepseek-flash \
RUSTY_CREW_DEN_ROUTER_MAX_TOKENS=64 \
npm run smoke:delegated-slice
```

The smoke defaults to `deepseek-flash` when available, then tries other
non-codex local den-router routes. Codex-backed routes are supported through the
Responses API mapping, but non-codex chat-completion routes are preferred for
this tiny smoke because they are faster and simpler.

## Expected Output

The command prints JSON with:

- planner brain event types and `request_delegation`;
- delegated worker session id;
- worker brain event types and `deliver_completion`;
- Den projection summaries;
- persistence counts for `sessions`, `worker_runs`, and `completion_packets`.

The expected persistence counts are:

```json
{
  "sessions": 2,
  "workerRuns": 1,
  "completionPackets": 1
}
```

## Notes

This smoke uses local diagnostic native-bridge helpers to create sessions,
route one operator message, project body state, submit action JSON, and count
rows. Those helpers are for local proof and development; production TS should
still treat worker spawning and wake scheduling as Rust-owned internals rather
than manifest operations.
