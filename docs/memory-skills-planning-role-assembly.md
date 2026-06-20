# Memory Skills Planning Role Assembly

Status: Implementation contract for task 2908

Date: 2026-06-20

## Scope

Memory, skills, and planning tools are model-callable utilities, but they also
need a coherent prompt surface. `buildProfileRoleAssembly` now accepts selected
context sections for:

- Den Memories guidance;
- dense profile memory records;
- planning guidance, including session-local todos, session search, and runtime
  counter guidance;
- selected skill summaries or full skill bodies.

The assembly function remains synchronous. Callers fetch Den or runtime memory
through the relevant adapter/native bridge first, select what should be prompt
visible, and pass the rendered context into assembly.

## Section Order

The deterministic role instruction order is:

1. Profile
2. Profile Instructions
3. Den Memory
4. Dense Profile Memory
5. Selected Skills
6. Tool Inventory
7. Planning Context
8. Runtime
9. Additional Instructions

Profile/system prompt text keeps precedence. Memory and planning sections are
context, not project truth.

## Render Helpers

`profile-role-assembly` exports:

- `renderDenMemoryContext`
- `renderDenseProfileMemoryContext`
- `renderPlanningContext`

Dense profile memory rendering is capped by record count and content length.
Planning context can include the existing `renderSessionTodoContext` output plus
guidance for `session_search` and `counter_reset`.

## Verification

`npm run smoke:profile-role-assembly` covers the ordering and proves the
assembled prompt reaches the pi-agent brain wrapper.
