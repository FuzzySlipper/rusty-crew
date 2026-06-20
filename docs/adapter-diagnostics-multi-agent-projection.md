# Adapter Diagnostics Multi-Agent Projection

Rusty Crew adapters must report health per binding/surface. They should not collapse runtime state into one global “current adapter agent,” because one degraded channel or MCP server must not degrade unrelated agents.

## Projection Inputs

`buildAdapterDiagnosticsProjection` consumes read-only facts:

- channel binding records;
- channel membership, presence, and subscription diagnostics;
- channel projection failure records;
- Den adapter status counters;
- MCP binding records;
- MCP surface diagnostics;
- MCP reload history.

The projection does not own transport state. It summarizes state already owned by adapters, the MCP surface manager, or Rust persistence.

## Channel Health

Channel diagnostics are reported per `bindingId` with:

- agent/session/profile ids;
- membership status;
- presence status and staleness;
- Rust subscription status;
- dropped projection count;
- last projection or subscription error.

A projection failure marks only that binding degraded. Other bindings for other agents can remain active.

## MCP Health

MCP diagnostics are reported per `bindingId` with:

- agent/session/profile ids;
- transport and server names;
- tool profile key;
- discovered tool revision;
- reconnect attempts;
- reload collision/discovery issue counts;
- optional server failures;
- last error.

A failed MCP surface or reload is scoped to its binding. Healthy MCP surfaces for other agents should remain active.

## Guardrail

The smoke `smoke:adapter-diagnostics` proves:

- two channel bindings can be summarized independently;
- one dropped channel projection degrades only the affected binding;
- two MCP surfaces can be summarized independently;
- one degraded MCP transport/reload degrades only the affected surface;
- internal Rust agent routing still accepts messages while adapter diagnostics are degraded.
