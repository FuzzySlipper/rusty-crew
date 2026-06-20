# Planning Runtime Wake Proof

Status: Proof note for task 2911

Date: 2026-06-20

## Scenario

`npm run smoke:planning-runtime-wake` proves the planning/runtime-state portion
of the 2818 tool family through the normal Rusty Crew wake path:

- native engine initialization;
- Rust session creation;
- pre-seeded persisted message/session history;
- profile-driven registry selection for `todo`, `session_search`, and
  `counter_reset`;
- brain registration through the production native bridge;
- role assembly with planning guidance;
- a brain wake that calls the selected tools through the Pi tool surface.

## Expected Runtime Effects

The proof verifies:

- `todo` writes bounded session-local planning state;
- a subsequent role assembly renders that todo state without treating it as Den
  task truth;
- `session_search` returns bounded Rust-owned message history;
- `counter_reset` summarizes runtime counters and resets the runtime `messages`
  derived counter to zero;
- one completion packet is delivered;
- eight tool-call history rows and eight observed tool events are persisted for
  four tool calls.

The search history remains in runtime facts after counter reset. Reset only
updates the derived counter projection.
