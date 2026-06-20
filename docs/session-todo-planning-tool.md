# Session Todo Planning Tool

Task: Den `2905`

Rusty Crew now has a `todo` brain tool for bounded session-local planning
state. This is intentionally separate from Den tasks and project truth.

## Scope

The first implementation is restart-bounded TypeScript-local state. It is
session-scoped, capped, and may have explicit TTL expiry. It is useful for a
brain to keep lightweight local planning notes during a session, not for durable
work tracking.

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

The in-memory store enforces a max item cap and rejects invalid items. `ttlMs`
can be supplied on replace/merge to make the state expire aggressively.

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
