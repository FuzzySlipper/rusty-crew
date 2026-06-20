# Session Search Brain Tool

Task: Den `2904`

Rusty Crew now exposes `session_search` as a brain-island tool over Rust-owned
runtime search history.

## Scope

`session_search` searches the `CoordinationStore` runtime FTS index. It does not
search Den tasks, Den documents, Den memories, or arbitrary product data.

Indexed rows currently include:

- routed agent messages, scoped by participating agent ids;
- immutable session configuration snapshots, scoped by session/profile/agent.

## Tool Behavior

The tool accepts:

- `query`
- optional `rowType`: `message`, `queue_message`, or `session`
- optional runtime filters: `sessionId`, `agentId`, `instanceId`, `taskId`,
  `eventKind`, `recordedAfter`, `recordedBefore`
- bounded `limit`

Results are normalized into stable agent-facing records:

- row type and row key
- sequence when applicable
- runtime identity fields
- recorded timestamp
- title
- bounded body snippet with `truncated`

## Native Bridge

The TypeScript tool calls `NativeBridgeModule.searchRuntime`, which maps through
the NAPI bridge to `CoreEngine::search_runtime` and then to the
`CoordinationStore::search_runtime` API from task `2872`.

The tool fails closed with `runtime_search_client_unavailable` when no runtime
search client is configured.

## Verification

`npm run smoke:session-search-tool` creates real runtime sessions and routed
messages, then proves:

- message search over Rust-owned runtime history;
- session search over immutable session snapshots;
- bounded snippets;
- missing-client denial.
