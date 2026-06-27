# Session Todo Planning Tool

Task: Den `2905`

Rusty Crew now has a `todo` brain tool for bounded session-local planning
state. This is intentionally separate from Den tasks and project truth.

## Scope

The tool stores session-scoped, capped planning notes with optional explicit TTL
expiry. In service mode the store is file-backed under
`<dataDir>/data/session-todos`, so notes survive normal process restarts while
remaining bounded scratch state. Non-service/test callers can still use the
in-memory store.

This is useful for a brain to keep lightweight local planning notes during a
session. It is not durable work tracking and must not be treated as Den task or
project truth.

## Tool Behavior

Supported actions:

- `read`: return the current session todo state.
- `replace`: replace the whole session todo list.
- `merge`: merge incoming items by id, preserving other items.

Todo statuses:

- `pending`
- `in_progress`
- `done`
- `blocked`
- `cancelled`

Both stores enforce a max item cap and reject invalid items. `ttlMs` can be
supplied on replace/merge to make the state expire aggressively. Expired or
corrupt file-backed session todo files are removed and read back as an empty
state for that session.

## Context Injection

`renderSessionTodoContext` renders the current list as a role/context section
that explicitly says the list is session-local and not Den task truth.

`buildProfileRoleAssembly` accepts an optional `todoContext` string and includes
it as a section in role instructions when supplied.

## Verification

`npm run smoke:todo-tool` covers:

- empty read
- replace
- merge-by-id
- cap rejection
- context-message rendering
- profile role assembly injection
- TTL expiry
- file-backed restart survival
