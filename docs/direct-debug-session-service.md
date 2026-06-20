# Direct Debug Session Service

Task: Den `2958`

Rusty Crew now has a direct-debug service boundary for inspecting a session without requiring debug clients to reach into runtime internals or route through Den Channels.

## Responsibilities

The service projects:

- current session identity, status, turn count, workdir, and selected tools
- runtime diagnostics for the session
- tool/context diagnostics when registry inputs are available
- role assembly summaries without raw prompt text by default
- pending message summaries
- recent activity summaries
- direct-turn control availability

## Prompt And Message Safety

Raw system prompt and instruction text are omitted by default. The service returns hashes, lengths, section headings, skill metadata, and bounded message previews only when requested.

Basic secret redaction is applied to debug previews and direct-turn input:

- `token=...`
- `api_key=...`
- `password=...`
- `secret=...`
- `Bearer ...`

This is intentionally conservative. Debug consumers should still treat the output as operator-only diagnostics.

## Direct Turns

Direct turn injection is disabled unless `allowDirectTurnInjection` is set and a `DirectDebugTurnExecutor` is configured.

When enabled, the service does not deliver a channel message. It passes an explicit `source: "direct_debug"` request to the configured executor with request/idempotency metadata. The executor is responsible for routing through the normal wake/control policy.

This keeps direct debug separate from Den Channels conversation delivery and avoids resurrecting or reclassifying channel messages.

## Verification

`npm run smoke:direct-debug-service` covers:

- sanitized session inspection
- prompt omission by default
- pending message preview redaction
- tool/context diagnostics projection
- disabled direct-turn rejection
- accepted direct-turn executor call with `source: "direct_debug"`
