# New Session Lifecycle

Status: implementation note for task 2955

Rusty Crew models `/new` as a lifecycle boundary: archive the current session and create a distinct fresh session. It is not an in-place prompt/context reset.

The implementation lives in `createNewSessionLifecycleExecutor` in `@rusty-crew/brain-island`. It is designed to be mounted as the `newSession` method of `AdminControlExecutor`, so slash commands and admin controls share the same guarded control path.

## Sequence

The executor:

1. Loads the current session template from typed runtime/query state.
2. Generates a distinct new session ID.
3. Fails before side effects if the new ID equals the old ID.
4. Fails before side effects if channel context exists but no explicit rebind handler is configured.
5. Archives the current session with reason code `slash_command_new` or the supplied reason code.
6. Creates the fresh session from the relevant agent/profile/session template.
7. Rebinds channel context deliberately when a channel binding is present.
8. Returns old/new session IDs and rebind status.

Durable runtime history is not deleted. Archived messages/history remain governed by retention/search policy.

## Audit And Observation

The executor can emit lifecycle audit phases:

- `template_loaded`
- `archive_started`
- `archived`
- `create_started`
- `created`
- `binding_rebind_started`
- `binding_rebound`

When configured with `AgentActivityObservationProducer`, it emits display-only session lifecycle breadcrumbs:

- `agent_session_stopped` for the archived session;
- `agent_session_started` for the fresh session.

These observation events do not drive runtime behavior.

## Rebind Rule

If the loaded session template includes `channelBindingId` or `channelId`, `/new` requires an explicit rebind handler. This prevents the dangerous half-state where a conversation appears fresh but old queue items, tool state, MCP surfaces, or channel bindings are still attached to the same executable session.

## Smoke Coverage

`npm run smoke:new-session-lifecycle` verifies:

- `/new` flows through the guarded admin control route;
- old and new session IDs are distinct;
- archive occurs before create;
- channel binding rebind is explicit;
- lifecycle audit phases are emitted;
- old/new session observation events are emitted with distinct session keys;
- duplicate new session IDs fail before side effects;
- missing rebind handler fails before side effects when channel context exists.
