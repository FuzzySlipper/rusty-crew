# ADR 0013: Rust Projects Wake State, The Bridge Owns Buffers

Status: Accepted

Date: 2026-06-20

## Context

A production brain wake crosses two boundaries:

- Rust owns deterministic runtime state and wake eligibility.
- TypeScript owns brain execution, LLM calls, tools, and adapter-facing code.

The bridge already has a `RuntimeBufferHandle` protocol for large payloads.
`BrainWakeRequest` carries handles for `body_state`, `system_prompt`, and
`role_assembly`; TypeScript hydrates those buffers and releases every handle.

The remaining open question is who assembles the three payloads before the
bridge creates buffer leases.

## Decision

Wake payload ownership is split by semantic authority:

- `body_state`: produced by Rust from `CoreEngine::prepare_body_state_for_wake`
  for production wakes. Rust owns session state, pending messages, recent
  events, child completions, fan-out groups, and body delta policy.
- `system_prompt`: resolved from the selected brain/profile binding. It is
  profile input, not scheduler logic. Until full profile assembly is complete,
  a minimal registered prompt string may be used, but it must come through the
  brain/profile registration path.
- `role_assembly`: produced by the profile/role assembly layer as a
  `BrainRoleAssembly`-compatible JSON payload. The scheduler consumes this
  output; it does not invent role semantics.

The bridge owns runtime buffer creation, leasing, hydration, and release. Core
coordination crates must not depend on napi or TypeScript callback types.

## Production Flow

1. The Rust scheduler selects a wakeable session and brain/profile binding.
2. Rust projects the wake `BodyState`.
3. The profile/role assembly layer supplies prompt text and role assembly JSON.
4. The bridge stores the three payloads in `RuntimeBufferStore` and creates a
   `BrainWakeRequest`.
5. TypeScript hydrates the request through `wakeBrainFromBridgeRequest`.
6. TypeScript releases each buffer exactly once before returning the brain
   result.

Diagnostic helpers may still accept caller-provided buffers for tests, but the
production wake loop must use projected body state and registered profile data.

## Guardrails

- Do not serialize body state in TypeScript by re-querying storage.
- Do not hard-code system prompts in scheduler code.
- Do not attach role assembly behavior to `wake_brain` transport glue.
- Do not add a second buffer protocol for large wake payloads.
- Do not retain hydrated wake buffers after the wake returns.

## Consequences

This preserves deterministic state ownership while keeping prompt and role
composition close to the brain/profile surface. The first implementation can be
minimal, but it must keep the owner lines visible so later pi-profile assembly
does not require reshaping the wake transport.
