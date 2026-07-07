# Roleplay Narrator Rust Strategy Plan

Status: active boundary note for the Rust-owned narrator FSM
Date: 2026-07-07

Rusty Crew's first Rust pi-agent cutover intentionally left the
`roleplay_narrator` strategy in TypeScript. That was the right first boundary:
the cutover proved the Rust brain loop, provider stream, and neutral tool bridge
without also rewriting roleplay sequencing.

This note defines the next boundary. Narrator sequencing should move to Rust,
but Rust should not call back into TypeScript brain implementations directly.
The durable shape is a Rust-owned deterministic finite-state machine (FSM) plus
a TypeScript executor.

## Current Shape

`crates/roleplay/roleplay-core` owns the narrator FSM and instruction builders.
`ts/packages/brain-island/src/narrator-brain.ts` is now an executor over Rust
phase plans.

Rust owns:

- phase order: explore, compose, optional compose draft, review, final compose;
- mandatory explore tool planning for `get_scene_state`, `recall_lore`, and
  conditional auto-capture layer lookup;
- mandatory locket/crest auto-capture request planning;
- allowed tool sets for explore, compose, draft, and review;
- instruction construction for explore, compose, and review;
- review feedback interpretation and max-cycle enforcement.

TypeScript owns:

- phase-change event emission;
- local/MCP tool resolution and execution for Rust-planned tool requests;
- Rust pi-agent phase wake invocation;
- chat/Rusty View event projection and completion action plumbing.

The phase brain invocation and tool execution remain TypeScript executor work.
The deterministic phase plan and instruction logic do not.

## Boundary

Rust owns the roleplay narrator FSM in `crates/roleplay/roleplay-core`.

Rust owns:

- narrator phase order and terminal state;
- allowed tool sets per phase;
- mandatory prelude tool request planning;
- auto-capture request planning;
- instruction construction for explore, compose, draft, and review phases;
- review feedback classification and max-cycle guard;
- typed state/result DTOs used by the bridge.

TypeScript owns:

- loading profile/config/tool context;
- resolving and executing local/MCP tools;
- invoking Rust pi-agent phase wakes;
- submitting `phase_change`, tool, reasoning, text, and final message events to
  existing chat/Rusty View surfaces;
- HTTP/admin/roleplay route envelopes.

No hidden TypeScript fallback should remain after the cutover. If the Rust FSM
cannot produce a valid next phase, the narrator turn should fail visibly rather
than silently switching to legacy TS sequencing.

## FSM Shape

The FSM should be pure and serializable across napi:

```text
start
  -> mandatory_prelude_requests
  -> explore_phase(instructions, allowed_tools)
  -> compose_phase(instructions, allowed_tools)
```

When review is enabled:

```text
start
  -> mandatory_prelude_requests
  -> explore_phase
  -> compose_draft_phase
  -> review_phase
  -> compose_draft_phase ... while review requests revision and cycles remain
  -> final_compose_phase
  -> done
```

The TypeScript executor should pass phase outputs back as plain text and tool
observation summaries. Rust should return the next phase plan.

## DTO Guidelines

Bridge DTOs should stay boring:

- `phase`: `explore | compose | compose_draft | review | done`;
- `instructions`: full instruction text for the phase;
- `allowedTools`: string array;
- `mandatoryToolRequests`: array of `{ toolName, paramsJson }`;
- `preludeObservations`: array of `{ toolName, ok, summary, detailsJson? }`;
- `sceneBrief`: text extracted from explore output;
- `reviewFeedback`: text extracted from review output;
- `reviewCycle`: integer;
- `maxReviewCycles`: integer;
- `terminal`: boolean.

Tool request parameters should be emitted by Rust as data. Agents/providers
should not be asked to emit JSON handoff shapes for this path.

## Why Not A Rust Brain That Calls TS

Putting the whole narrator executor in Rust would require Rust to call a
TypeScript phase brain, then receive streaming events, then call more TypeScript
tools. That creates an awkward Rust -> TS -> Rust -> TS loop and would move the
wrong authority first.

The FSM split still moves the durable sequencing logic into Rust while keeping
the existing neutral brain/tool streaming path intact. It also gives a clean
reviewable transition:

1. Rust pure domain behavior.
2. Bridge DTOs.
3. TypeScript executor cutover.
4. Live roleplay certification.

## Task Series

- #4603: design the Rust FSM contract and bridge DTOs.
- #4604: implement the pure `roleplay-core` narrator FSM and tests.
- #4605: expose the FSM through the native bridge and a TS typed wrapper.
- #4606: cut `narrator-brain.ts` over to the Rust FSM executor and remove
  duplicate TS deterministic logic.
- #4607: live certify Rust-owned narrator sequencing through the debug service
  and Rusty View.

## Validation Gates

Deterministic gates:

- `cargo test -p rusty-crew-roleplay-core`
- bridge parity/fingerprint/validation gates after the native operation lands
- `npm run smoke:roleplay-narrator-brain -w @rusty-crew/brain-island`
- `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island`

Live gate:

- debug service `/home/system/rusty-crew-debug` on port `9348`;
- real provider-backed roleplay narrator profile;
- Rusty View rendered proof with phase, tool, reasoning/text, and final clean
  narrative output.
