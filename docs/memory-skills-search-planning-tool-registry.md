# Memory, Skills, Search, And Planning Tool Registry

Task: Den `2897`

Rusty Crew now registers the `2818` tool family in the canonical
`brain-island` tool registry. The entries are metadata/selection contracts for
future implementation tasks; model-callable tools still need executor modules
before profiles should select them in production.

## Registered Tools

Den Memories:

- `den_memory_recall`
- `den_memory_read`
- `den_memory_search`
- `den_memory_store`
- `den_memory_propose`

Rusty Crew memory:

- `dense_profile_memory`

Skills:

- `skills_list`
- `skill_view`
- `skill_manage`

Planning/search/counters/governance:

- `todo`
- `session_search`
- `counter_reset`
- `curator_execute`

## Toolsets

The new entries use explicit toolsets so profiles can request narrow slices:

- `memory_den_read`
- `memory_den_write`
- `memory_profile`
- `skills_read`
- `skills_manage`
- `planning_session`
- `planning_privileged`
- `runtime_search`
- `runtime_counters`
- `curator_governance`

Memory write, skill management, counter reset, and curator execution are not
accidentally included in read-only toolsets.

## Safety And Authority

Read-only Den memory and skills tools are marked read-only. Den memory access is
also marked `network_access`; Den memory write/propose tools are
`external_write`.

Dense profile memory, todo, session search, counter reset, and curator execute
are marked `coordination_action` because they relate to Rusty Crew runtime or
governance state. This does not make TypeScript the authority; it makes the
selection and diagnostics surface explicit.

## Disabled-State Diagnostics

`ToolInventoryRequest` now accepts optional reason maps for profile, session,
and resource denials:

- `profileDeniedReasons`
- `sessionDeniedReasons`
- `resourceDeniedReasons`

This lets future loaders explain disabled states such as:

- Den Memories endpoint unavailable;
- dense profile memory persistence unavailable;
- runtime search unavailable;
- counter service unavailable;
- curator disabled by policy;
- skill root not writable.

The selected/denied/missing/collided inventory path remains the supported way to
explain why an agent has or lacks a memory, skill, search, or planning tool.

## Verification

`npm run smoke:tool-registry` proves selection and denial behavior for the new
toolsets. `npm run smoke:tool-registry-diagnostics` proves the expanded default
registry remains valid and visible through diagnostics.
