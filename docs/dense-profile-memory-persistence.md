# Dense Profile Memory Persistence

Task: Den `2900`

Rusty Crew now has a Rust-owned persistence model for dense profile memory.
This is separate from Den Memories, skill files, and session-local planning
state.

## Shape

Dense profile memory lives behind `CoordinationStore` APIs in
`crates/core/core-persistence`. The storage table is `profile_memories`, added
by schema migration 13.

Records are scoped by:

- `profile_id`
- target type and target id
- memory key

Targets are explicit:

- `ProfileMemoryTarget::Profile`: compact memory for the profile itself.
- `ProfileMemoryTarget::User(String)`: compact profile-owned memory about a
  user or external participant.

The same key may exist for different profiles or different targets without
colliding.

## API

The store exposes repository-style methods:

- `list_profile_memory`
- `get_profile_memory`
- `add_profile_memory`
- `replace_profile_memory`
- `remove_profile_memory`

Writes are split intentionally. `add` fails when a record exists. `replace` and
`remove` require an expected revision. The revision is the write token: stale
updates fail with `ActionRejected` instead of silently replacing newer memory.

## Caps

`ProfileMemoryCaps` bounds:

- maximum records per profile
- maximum key bytes
- maximum content bytes

Caps are checked before mutation. They are part of the runtime API rather than a
SQLite-specific behavior, so a future storage backend can preserve the same
contract.

## Boundaries

Dense profile memory is runtime/profile state. It is not Den product memory, it
does not write skill files, and it does not replace Den tasks or documents.

The implementation keeps SQLite details inside `core-persistence`; callers use
typed Rust methods and records.

## Verification

`cargo test -p rusty-crew-core-persistence` covers:

- migration from old schemas to schema version 13
- table and index creation
- profile isolation
- profile-target versus user-target separation
- add/replace/remove behavior
- revision mismatch rejection
- record caps
- content-size caps
