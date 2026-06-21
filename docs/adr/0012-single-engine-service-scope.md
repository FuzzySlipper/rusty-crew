# ADR 0012: One Engine Per Service Process

Status: Accepted

Date: 2026-06-20

## Context

The parity audit left an open question about whether Rusty Crew should support
multiple runtime engines inside one process through an `EngineRegistry`.

In Rusty Crew, an engine means the Rust `CoreEngine` coordination instance:
session registry, in-process bus, persistence handle, scheduler state,
body-state projection, action validation, and deterministic runtime
transitions. It is not an LLM engine, browser process, MCP connection,
den-channels transport, or TypeScript tool executor.

Current code already has two different scopes:

- `CoreEngine` allocates an `EngineHandle`.
- `NativeBridge` stores a single initialized `CoreEngine` and rejects a second
  `initialize_engine` call.

The unified architecture describes a single service runtime that hosts many
agents, sessions, profiles, adapters, and tool surfaces through one Rust-owned
coordination substrate.

## Decision

Rusty Crew v1 uses one `CoreEngine` per service process.

Multi-agent hosting is handled inside that engine:

- many full, delegated, and future worker sessions;
- many profile ids and selected `ToolProfile`s;
- many platform adapter bindings;
- many brain implementation registrations;
- many agent identities and channel/MCP/browser resources scoped by session or
  profile.

An `EngineRegistry` is not part of v1. Bridge, admin, scheduler, diagnostics,
and adapter calls target the one active service engine.

## Future Reopening Triggers

Revisit an in-process `EngineRegistry` only if there is a concrete requirement
for hard tenancy or lifecycle isolation inside one OS process, such as:

- hosting unrelated user tenants whose runtime stores must be independently
  opened, drained, and shut down;
- blue/green engine migration in one long-lived process;
- multiple persistence roots that must be active simultaneously;
- test harnesses that cannot isolate via separate processes or temp dirs;
- admin APIs that must route every command by engine id.

If reopened, the registry must route all bridge/admin calls explicitly by
engine handle and prevent cross-engine sessions, subscriptions, buffers, and
adapter bindings.

## Terminology

Use `CoreEngine` or `runtime engine` when referring to Rust coordination. Use
`LLM provider`, `model`, `browser session`, `MCP client`, or `adapter` for the
other subsystems. Avoid bare "engine" in new docs unless the surrounding
section clearly means the Rust runtime engine.

## Consequences

This keeps the v1 bridge and scheduler simple while leaving expansion space.
Dozens of agents do not require dozens of engines; they require correct
session, profile, adapter, queue, and resource scoping inside the one service
engine.
