# Service App Decomposition Inventory

Status: implementation inventory for task 5301
Date: 2026-07-09

## Purpose

`ts/packages/brain-island/src/service-app.ts` remains the largest TypeScript
composition surface in Rusty Crew. The file is allowed to compose service
dependencies, but it should not keep absorbing every route family, wake helper,
diagnostic projection, and runtime mutation path.

This inventory assigns the current concerns to extraction lanes. It is not a
claim that all lanes are complete.

## Current Shape

Before task #5301's first extraction, `service-app.ts` was roughly 11.8k lines.
It mixed:

- HTTP route dispatch and per-route parsing;
- profile registry and runtime config mutation helpers;
- MCP server registry mutation helpers;
- direct debug routes and diagnostics assembly;
- Den and Telegram connector lifecycle;
- runtime config reload/rebuild/session replacement;
- Rusty View chat API operations and event-stream projection;
- slash command execution;
- wake dispatch, provider request setup, and post-turn maintenance;
- scheduler heartbeat and background review loops;
- service shutdown and low-level response/header helpers.

The decomposition target is not "no TypeScript." TypeScript remains the right
home for HTTP envelopes, provider/client glue, adapter calls, and service-host
composition. The target is to move route families and policy-adjacent helpers
out of the central file into named modules with explicit dependencies.

## Landed Slice

### MCP server registry routes

Task #5301 extracted the `/v1/admin/mcp/servers` route family into
`ts/packages/brain-island/src/service-mcp-server-registry-routes.ts`.

The extracted module owns:

- collection and item method handling;
- MCP server write-body parsing and validation;
- runtime-managed server create/update/delete behavior;
- delete denials when active MCP bindings still reference a runtime server;
- collection catalog projection after mutation.

`service-app.ts` still owns:

- reading the HTTP body;
- supplying the runtime config mutation queue;
- reading/writing the service config file;
- applying the runtime config reload after mutation;
- passing current config/runtimeConfig through explicit callbacks.

The extraction keeps the existing runtime-config draft parser on the same
`mcpServerWriteFromBody` helper so there is one parser for MCP server records.

Validation for the slice:

```sh
npm run typecheck
npm run smoke:mcp-server-registry-routes -w @rusty-crew/brain-island
```

## Remaining Extraction Lanes

These lanes should become child tasks or implementation slices. Keep each slice
behavior-preserving and covered by a focused smoke/API check.

### Profile and runtime config mutations

Current concerns:

- profile registry runtime-config write planning;
- profile file synchronization;
- service profile create/decommission/delete;
- runtime config draft planning and atomic writes;
- runtime rebuild, replacement sessions, and profile registry session ref
  replacement.

Target:

- Route/admin parsing in focused service modules.
- Rust planners remain the authority for runtime-affecting validation.
- `service-app.ts` supplies process-level callbacks and current state.

### Direct debug and diagnostics routes

Current concerns:

- `/v1/debug/...` route parsing;
- direct debug context assembly;
- provider request debug readback;
- context compaction debug injection;
- broad diagnostics assembly.

Target:

- Move debug route parsing and readback helpers into a service debug route
  module.
- Keep diagnostic projections as read-only views over already-owned state.

### Adapter lifecycle and channel ingress

Current concerns:

- Den successor Gateway startup/projection/drain;
- Den conversation channel ensure/diagnostics;
- Telegram connector lifecycle;
- channel ingress route planning and dynamic delivery channel recording.

Target:

- Adapter lifecycle modules own platform glue.
- Rust route planning remains the authority for channel ingress decisions.
- Observability failures stay explicit but non-blocking.

### Rusty View chat projection and command routes

Current concerns:

- chat message submission;
- chat read-model and event-log projection;
- branch/slot/variant/attachment/data-bank operations;
- slash command routing and output envelopes;
- SSE subscribers and replay.

Target:

- Keep browser/API envelope code in TS modules.
- Move durable chat mutation and projection decisions behind Rust/domain calls
  where not already done.
- Avoid mixing command execution with wake dispatch internals.

### Wake dispatch and post-turn maintenance

Current concerns:

- wake event drain and suppression;
- active wake tracking;
- provider request construction and timeout handling;
- chat event append/projection;
- tool observation publication;
- completion packet projection;
- post-turn maintenance, activity digest persistence, and curator follow-up.

Target:

- Extract wake orchestration into a named service wake module with explicit
  ports for bridge, profile loading, provider debug store, chat append, and
  observation sinks.
- Keep provider/tool execution glue in TS where needed, but do not let wake
  policy become invisible route logic.

### Scheduler and background loops

Current concerns:

- scheduler heartbeat execution;
- host job payload parsing;
- background memory skill review;
- curator lifecycle transitions;
- maintenance reporting.

Target:

- Keep loop hosting in service-host/background modules.
- Rust-owned scheduler and maintenance decisions should be visible through
  native/bridge calls.

### Low-level HTTP helpers

Current concerns:

- response writing;
- CORS headers;
- query parsing;
- body parsing;
- auth/header helpers;
- shared object/string validation helpers.

Target:

- Move generic HTTP helpers only when another route extraction benefits.
- Avoid creating a broad `utils.ts` dumping ground.

## Follow-Up Rule

Every extraction should record which bucket it achieves:

- `migrated` only when production behavior now depends on Rust/codegen
  authority;
- `ratcheted` when a test/check prevents further sprawl;
- `certified-current-boundary` when TS remains the intentional owner;
- `planned` when only a lane is named.

Use the `migrated`, `ratcheted`, `certified-current-boundary`, and `planned`
vocabulary above when describing follow-up work.
