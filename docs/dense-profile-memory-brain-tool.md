# Dense Profile Memory Brain Tool

Task: Den `2901`

Rusty Crew now exposes `dense_profile_memory` as a brain-island tool backed by
the Rust-owned dense profile memory store.

## Tool Behavior

The tool supports five actions:

- `list`: list compact memory for the current profile, optionally filtered by
  target.
- `read`: read one memory record by key.
- `add`: create a new memory record.
- `replace`: replace a record using an expected revision write token.
- `remove`: remove a record using an expected revision write token.

By default the target is the profile itself. A caller may target a user with
`targetType: "user"` and a `targetId`.

The tool description explicitly tells agents not to store task progress, todos,
temporary outcomes, or Den product facts in dense profile memory.

## Policy

The tool context has a mode:

- `off`: all calls are denied.
- `read_only`: `list` and `read` are allowed; writes are denied.
- `read_write`: all actions are allowed.

The profile id is resolved from tool params, explicit context, or the current
session. Missing profile identity fails closed.

## Native Bridge

The TypeScript tool calls a narrow client surface implemented by
`@rusty-crew/native-bridge`:

- `listProfileMemory`
- `getProfileMemory`
- `addProfileMemory`
- `replaceProfileMemory`
- `removeProfileMemory`

The native bridge maps those calls through `CoreEngine` to the
`CoordinationStore` APIs from task `2900`. The smoke test rebuilds the local NAPI
artifact and proves records survive shutdown and reinitialization with the same
engine data directory.

## Verification

`npm run smoke:dense-profile-memory-tool` covers:

- read-only write denial
- add
- replace with revision increment
- stale replacement failure
- user target storage
- shutdown/reinitialize restart readback
- list after restart
- remove with expected revision
