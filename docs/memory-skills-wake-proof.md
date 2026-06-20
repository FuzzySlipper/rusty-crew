# Memory Skills Wake Proof

Status: Proof note for task 2910

Date: 2026-06-20

## Scenario

`npm run smoke:memory-skills-wake` proves the memory/skills portion of the 2818
tool family through the normal Rusty Crew wake path:

- native engine initialization;
- Rust session creation;
- profile-driven registry selection for Den memory, dense profile memory, and
  skill read tools;
- brain registration through the production native bridge;
- role assembly containing Den memory guidance, dense profile memory context,
  and selected skill context;
- fake Den memory client for read-only recall;
- Rust-owned dense profile memory read through `dense_profile_memory`;
- skill listing and viewing through filesystem-backed skill tools.

## Expected Runtime Effects

The proof produces durable runtime facts:

- one completion packet;
- eight tool-call history rows for four tool calls;
- eight observed brain tool events on the event subscription;
- tool/context diagnostics for the memory/skills surfaces.

The proof avoids direct diagnostic brain helpers for wake execution. It uses the
same native `registerBrainImplementationRuntime`, `buildBrainWakeRequest`, and
`wakeBrain` path used by production bridge wakes.
