# ADR 0014: ToolProfile Is The Session Contract, TypeScript Is The Executor

Status: Accepted

Date: 2026-06-20

## Context

Rusty Crew tools are split across Rust and TypeScript. TypeScript has the
model-callable implementations and registry. Rust has durable session state,
profile binding, wake context, resource limits, and audit records.

The parity audit asked how `ToolProfile`s should be enforced. Current code now
has the main pieces:

- `SessionState` carries a selected `ToolProfile`.
- brain implementation registration carries a profile id and `ToolProfile`.
- Rust validates duplicate tool names at registration/session boundaries.
- delegated sessions resolve the selected profile through Rust's profile mirror.
- `resolveToolSession` filters concrete TypeScript tools back to the Rust
  projected session `ToolProfile`.
- registry diagnostics flag duplicate, deprecated, shadowed, or missing tools.

## Decision

`ToolProfile` is the canonical per-session contract for which tools a brain is
allowed to see. Rust owns the selected contract and TypeScript owns concrete
execution.

Rust responsibilities:

- retain selected `ToolProfile` on every session;
- validate malformed or duplicate descriptors at registration boundaries;
- bind profile id, session id, and selected tool descriptors for audit;
- project `ToolProfile` in wake `BodyState`;
- validate action-level coordination effects such as delegation and completion;
- persist durable tool-call telemetry from brain events;
- enforce resource constraints that must survive restart, such as workdir,
  session archive, cancellation, timeout, and delegation depth.

TypeScript responsibilities:

- maintain the canonical tool registry and inventory diagnostics;
- load profile policy into requested toolsets/tools;
- convert selected registry entries into concrete Pi tool implementations;
- filter concrete implementations by the Rust-projected `ToolProfile`;
- refuse unavailable, duplicate, or not-requested implementations defensively;
- expose model schemas only for selected callable tools.

## Registry Rules

All model-callable tools must pass through the canonical registry. New tools
must not be injected by ad hoc per-agent arrays except in tests that are
explicitly proving a resolver path.

To prevent duplicate drift:

- canonical names are lower snake case;
- aliases are compatibility metadata, not parallel active tools;
- numbered variants such as `do_thing2` are not acceptable replacements;
- output-shape/category collisions require an explicit coexistence note;
- registry diagnostics must explain `selected`, `not_requested`,
  `profile_denied`, `session_denied`, `resource_denied`, `deprecated`,
  `missing`, `shadowed`, and `collision` outcomes.

## Enforcement Shape

The LLM never receives tools outside the session `ToolProfile`. TypeScript
filtering is the immediate model-facing enforcement. Rust is the durable
authority for what was selected and for coordination effects that tools request.

Ordinary local, web, browser, memory, skills, and MCP tools execute in
TypeScript. Tools that mutate Rust coordination state should emit structured
`BrainAction`s or future typed runtime commands rather than editing state
directly in TypeScript.

## Consequences

This avoids rebuilding pi-crew's worker-policy-first tool gate. Profiles and
session/resource constraints select tools; worker pools may later affect
capacity but must not become the default authority for full or prime agents.
