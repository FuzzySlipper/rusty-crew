# MCP Rust Telemetry And Resource Hooks

Rusty Crew keeps MCP client implementation in the TypeScript adapter layer. Rust owns durable coordination state, auditability, and policy visibility for each session/profile surface.

## Durable Tool Telemetry

MCP tool execution uses the normal `BrainEvent` tool-call path:

- `tool_call_started`
- `tool_call_finished`

Both events may carry `ToolCallMetadata`. For MCP calls the metadata records:

- MCP source marker
- adapter id
- binding id
- server names
- profile id
- tool profile key
- source MCP tool name
- catalog revision
- resource-policy facts

The metadata intentionally excludes tool arguments, raw results, credentials, endpoint secrets, and large payloads. Rust persists it in `tool_call_history.metadata_json` so audits can answer which session/profile used which MCP surface without replaying or storing private tool data.

## Resource Hooks

TypeScript evaluates MCP resource hooks before and around execution, then emits the decision through the same metadata object. The current hook facts are:

- tool-profile allow/deny
- timeout budget
- cancellation
- session archive cleanup

Denied or skipped tools should be represented as policy decisions, not resurrected as queued work. If a tool was never executed, do not emit a normal started/finished pair just to make it visible; use the decision metadata in the controlling flow or a future dedicated audit projection.

## Catalog Changes

MCP discovery/reload continues to emit `tool_catalog_changed` through the existing external-event payload route. The registry integration includes this payload in its report after discovery or reload, and Rust can observe it as an external event without conflating catalog drift with brain execution.

## Operational Notes

- Treat `metadata_json` as audit metadata, not a debugging dump.
- Prefer stable ids and revision strings over URLs or credentials.
- Keep MCP adapter changes isolated behind the MCP binding and registry integration boundaries.
- If future agents need to inspect expired/skipped MCP decisions, add a dedicated read-side tool instead of overloading execution queues.
