# ADR 0021: First-Class Brain Modules Behind The Neutral Wake Contract

Status: Accepted for task 3906

Date: 2026-07-01

## Context

Rusty Crew began with a simple architecture sentence: Rust owns deterministic
coordination, and TypeScript owns the brain island. That was accurate while the
only production brain was the TypeScript `@earendil-works/pi-agent-core` loop.

The code has since grown a direct Rust brain module under
`crates/brains/openai-responses`. That crate is wired into production wake
handling through the native bridge and exists for a real reason: the OpenAI
Responses API is not just a chat-completions variant. Response chaining,
provider state, reasoning content, tool event shape, and streaming behavior are
agent-loop concerns. A compatibility shim can hide some differences, but it
cannot preserve the full provider semantics or token behavior.

Rusty Crew already has the boundary that makes this safe:

- Rust coordination wakes a brain with a frozen `BodyState` snapshot.
- Brain implementations emit `BrainWakeStreamItem`s, `BrainEvent`s,
  `BrainAction`s, and provider-state updates.
- Rust ingests the stream, validates accepted actions, owns lifecycle effects,
  and persists coordination state.

The implementation language of a brain module should not be the boundary. The
wake contract should be the boundary.

## Decision

Rusty Crew treats brain modules as first-class implementations behind the
language-neutral wake/stream/action/provider-state contract.

TypeScript brain modules remain first-class. The current pi-agent brain owns the
pi package integration, model-callable tool adaptation, profile/role assembly,
and provider/tool behavior that naturally lives in TypeScript.

Rust brain modules are also first-class when they stay behind the approved
protocol surfaces. They may live under `crates/brains/` and depend on:

- `rusty-crew-core-protocol`;
- `rusty-crew-core-bridge-api`.

They must not depend on Rust coordination internals:

- `rusty-crew-core-engine`;
- `rusty-crew-core-session`;
- `rusty-crew-core-bus`;
- `rusty-crew-core-body`;
- `rusty-crew-core-persistence`;
- service-host, adapter, native-bridge implementation, or local config crates.

Brain modules own provider request construction, provider response parsing,
provider wire-state payload semantics, and mapping provider events into the
neutral brain stream. They do not own session lifecycle, wake scheduling, bus
routing, tool profile selection, action validation, delegation lifecycle,
coordination persistence, Den integration, MCP integration, platform adapters,
or service configuration.

## Boundary Contract

A brain module receives:

- wake id and session id;
- frozen body state;
- assembled prompt/role context;
- selected tool descriptors;
- optional provider state and absence reason;
- module/profile configuration that has already passed the service boundary.

A brain module emits:

- `started`, `text_delta`, `reasoning_delta`, `tool_call_started`,
  `tool_call_finished`, `provider_status`, and `finished` style events;
- terminal action batches or wake failures;
- provider-state output when the module uses provider state;
- transport/provider diagnostics that can be projected without becoming
  coordination authority.

Rust coordination remains authoritative for:

- deciding when to wake;
- freezing snapshots;
- ingesting stream items;
- validating and applying brain actions;
- routing internal messages;
- creating delegated sessions/workers;
- persisting coordination facts and projections;
- queuing next-wake deltas.

## Why Rust Brain Modules Exist

Direct Rust brain modules are useful when the provider loop benefits from
provider-native semantics that should not be squeezed through the pi-agent
runtime:

- Responses API replay and previous-response chaining;
- provider-state persistence and invalidation;
- reasoning content and encrypted reasoning payloads;
- provider-native tool call streams;
- capacity and correctness work that is easier to test in Rust;
- future providers whose loop shape is materially different from pi's current
  chat-completions-oriented abstraction.

This is not a decision to rewrite every brain in Rust. It is a decision to keep
the architecture modular enough that the best loop can be used for a profile or
provider without changing Rust coordination.

## Validation Gates

Rust brain modules require the same or stronger checks as TypeScript brain
modules:

- `npm run smoke:rust-crate-boundaries` must fail if a Rust brain reaches into
  coordination internals.
- Fake-client tests must cover terminal stream shape, provider-state behavior,
  tool calls, reasoning/text deltas, and error propagation.
- Bridge contract validation/checker work must cover any new or changed stream
  fields.
- User-facing chat behavior must be live-certified through Rusty View when a
  brain module affects streaming, reasoning, tools, commands, or visible chat
  behavior.

Deterministic tests prove code paths. Live certification proves the feature works
in the actual service/client/LLM loop.

## Consequences

Architecture wording should change from "TypeScript owns LLM calls" to "brain
modules can be TypeScript or Rust behind the neutral wake contract." TypeScript
continues to own the pi-agent brain, many model-callable tool implementations,
profile/role composition, MCP clients, and platform adapters. Rust continues to
own deterministic coordination.

The `crates/brains/README.md`, `docs/brain-wake-stream-protocol.md`, and
`docs/rust-brain-crate-firewall.md` are aligned with this ADR. Older prose that
describes TypeScript as the only LLM-call owner is historical and should be
updated or read through this ADR.

## Non-Goals

- Do not move platform adapters into Rust brain modules.
- Do not let Rust brain modules read or mutate coordination storage directly.
- Do not use Rust brain modules to bypass profile tool selection.
- Do not require all providers to use Rust brains.
- Do not keep TypeScript and Rust brain implementations behavior-compatible
  through hidden fallbacks when their provider semantics genuinely differ.

