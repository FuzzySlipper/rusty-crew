# MCP Client Transports And Lifecycle

Status: Initial implementation for task 2937

Date: 2026-06-20

Depends on:

- `per-agent-mcp-surface-architecture`
- `channel-mcp-binding-persistence`

## Purpose

Rusty Crew needs multiple independent MCP client surfaces in one service. The
MCP adapter owns client transport lifecycle in TypeScript, while Rust continues
to own session identity, profile/resource constraints, and final `ToolProfile`
audit.

The first implementation establishes the lifecycle shape without pulling tool
discovery and schema conversion forward from later tasks.

## Implementation Location

The lifecycle manager lives in:

`ts/packages/adapter-mcp/src/index.ts`

Shared TS contracts now include:

- `McpBindingRecord`
- `McpSurfaceIdentity`
- `McpSurfaceDiagnostics`
- `McpSurfaceStatus`
- `McpTransportKind`

These mirror the safe Rust persistence shape and intentionally avoid secrets.

## Surface Manager

`McpSurfaceManager` manages client state keyed by `bindingId`.

It supports:

- connect;
- reconnect;
- reload;
- disconnect;
- archive;
- shutdown;
- identity readback;
- diagnostics readback.

Each binding carries its own agent/session/profile identity, server selection,
endpoint ref, transport, and tool profile key. There is no global current MCP
client.

## Transport Factories

Transport behavior is pluggable through `McpTransportFactory`.

The initial manager recognizes whatever factory kinds are registered. The smoke
uses simulated factories for:

- `stdio`
- `streamable_http`

The real stdio and streamable-HTTP protocol clients should implement the same
factory/client interface in follow-up tasks.

## Degradation And Cleanup

Unsupported transports or exhausted connect attempts degrade the surface and
record a bounded diagnostic reason. Optional surfaces are marked from safe
diagnostic notes for now; later config loading can make optionality explicit.

Reload disconnects the previous client before reconnecting with the new binding.
Archive and shutdown disconnect clients and leave readback diagnostics in an
archived state.

## Covered Cases

`smoke:mcp` covers:

- independent stdio and streamable-HTTP surfaces;
- bounded retry after a simulated HTTP connection failure;
- optional surface flagging;
- identity readback per surface;
- unsupported transport degradation;
- reload updating server selection without mutating another surface;
- archive and shutdown cleanup.
