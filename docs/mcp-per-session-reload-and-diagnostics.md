# MCP Per-Session Reload And Diagnostics

Status: Initial implementation for task 2940

Date: 2026-06-20

Depends on:

- `mcp-client-transports-and-lifecycle`
- `mcp-tool-discovery-and-schema-conversion`
- `mcp-registry-integration-and-collision-policy`

## Purpose

MCP reload must be scoped to one binding/session/profile surface. Refreshing one
agent's MCP catalog should not mutate another agent's live surface, selected
tools, or diagnostics.

## Implementation Location

Reload coordination lives in:

`ts/packages/brain-island/src/mcp-surface-reload.ts`

It exports:

- `reloadMcpSurface`
- `McpSurfaceReloadReport`
- `McpToolDiff`

## Reload Flow

`reloadMcpSurface` coordinates the existing layers:

1. reload the specific binding through `McpSurfaceManager`;
2. stop before discovery if the surface cannot reconnect;
3. discover MCP tools from the reloaded surface;
4. integrate candidates with the canonical registry/inventory;
5. compute old/new/added/removed/unchanged tool names;
6. report collisions, discovery issues, optional failures, duration, requester,
   and reason metadata.

The report is diagnostic/admin shaped and safe to expose later. It carries
endpoint refs and bounded error strings, not raw secrets.

## Degradation

If connect/reload fails, the report status is `degraded`. Optional surface
failures are collected in `optionalServerFailures`, and discovery is not run
against a failed connection.

If registry validation fails after discovery, the report also degrades with a
registry-validation reason while keeping the validation details in the registry
report.

## Isolation

Reload operates on one `McpBindingRecord`. Other surfaces in the same manager
remain connected and keep their diagnostics. This is the critical guardrail
against returning to a global MCP client model.

## Covered Cases

`smoke:mcp-reload` covers:

- reload of one binding while another remains active;
- added/removed/unchanged tool diff;
- requested-by and reason metadata;
- catalog change payload through registry integration;
- optional surface degradation without discovery execution;
- duration and safe diagnostic reporting.
