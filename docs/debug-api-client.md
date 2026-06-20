# Debug API Client

Task: Den `2959`

Rusty Crew now has a typed TypeScript client for local admin/debug API consumers such as CLI tools, debug TUI views, and future operator dashboards.

## Methods

The client exposes typed methods for:

- diagnostics bundle and overview
- readiness
- sessions
- tools
- MCP surfaces
- channel bindings
- observation status
- metrics
- recent events
- direct-debug session context
- direct-debug turn request

This keeps debug UI code from hand-writing fetch calls and route strings.

## Transport Behavior

`createDebugApiClient` accepts:

- `baseUrl`
- optional bearer token
- optional fetch implementation
- timeout in milliseconds
- GET retry count

GET requests retry retryable local failures such as transient 5xx envelopes or aborted transport calls. POST requests are not retried by default, which keeps direct debug turn submission idempotency explicit.

API failures throw `DebugApiClientError` with:

- `code`
- HTTP `status`
- admin/debug `reasonCode`
- `retryable`

## Direct Debug Boundary

The client has direct-debug methods, but it does not treat direct debug as channel delivery. The server side still owns whether a direct turn is allowed and how it routes through wake policy.

## Verification

`npm run smoke:debug-api-client` uses a fake fetch/admin server to cover auth headers, diagnostics methods, direct-debug context, direct-turn POST behavior, retry on a transient GET failure, and clear error propagation.
