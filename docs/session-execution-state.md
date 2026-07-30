# Session Execution State

Rust owns the canonical execution projection for native brain sessions. This
projection answers whether a session is working now without changing the
session's durable lifecycle or asking TypeScript to infer activity from local
promises.

## Contract

`SessionExecutionState` separates three concerns:

- `lifecycleStatus`: whether the durable session is `live` or `archived`.
- `phase`: `idle`, `queued`, `active`, `waiting`, `paused`, or `cancelling`.
- `lastOutcome`: the latest terminal `completed`, `failed`, `cancelled`, or
  `interrupted` outcome when the current phase is idle.

The projection also carries its authoritative source, relevant wake and logical
turn identifiers, a stable reason code, a short summary, and timestamps. The
legacy `SessionState.status` remains protocol-visible, but Rust now derives its
`active` or `idle` value from this same projection so the two cannot disagree.

## Precedence

Rust derives execution state from durable logical-turn and runtime-activity
records in this order:

1. An archived session is always idle with an archived lifecycle.
2. A nonterminal logical turn controls queued, active, paused, and cancelling
   phases. Active tool work within a running logical turn projects `waiting`.
3. Otherwise, active runtime activity controls queued, active, or waiting.
4. With no active work, the newest terminal logical turn or root runtime
   activity supplies the last outcome; otherwise the session is simply idle.

Service restart interrupts activities owned by the prior service instance.
Hydrated sessions therefore return as idle with an `interrupted` last outcome
instead of being resurrected as active.

## API Surfaces

Chat session list/open responses include `session.execution`. Changes are also
persisted into the chat event stream as `session_execution_changed`, allowing a
client to update working state without polling or parsing assistant events.
Diagnostics and the legacy session status both consume the same Rust
projection. TypeScript must not overlay process-local wake state onto it.
