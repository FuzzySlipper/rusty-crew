# Memory Skills Planning Diagnostics

Status: Implementation contract for task 2909

Date: 2026-06-20

## Scope

Tool/context diagnostics now include a bounded `memorySkillsPlanning` section for
the memory, skills, search, todo, and counter tool family.

The report accepts safe status and count inputs from the caller. It does not
read raw memory bodies, dump skill bodies, expose Den credentials, or query
runtime persistence directly.

## Covered Surfaces

The summary covers:

- Den memory configuration, policy mode, client availability, and last safe
  error;
- skill root readability plus profile, loaded, pinned, protected, invalid, and
  missing skill counts;
- dense profile memory client availability, record count, configured cap, and
  cap-reached status;
- session search availability, indexed row count, last indexed timestamp, and
  last safe error;
- todo availability, item count, blocked item count, and expiry timestamp;
- runtime counter availability, reset permission, and bounded counter summary.

## Issue Codes

Diagnostics may emit these tool-family reason codes:

- `den_memory_unavailable`
- `skill_root_unavailable`
- `dense_profile_memory_unavailable`
- `session_search_unavailable`
- `todo_state_unavailable`
- `runtime_counter_unavailable`

Registry/inventory diagnostics still explain why individual tools are selected,
denied, missing, deprecated, or collided.

## Verification

`npm run smoke:tool-context-diagnostics` covers the summary shape, markdown, and
issue codes without leaking raw system prompt text.
