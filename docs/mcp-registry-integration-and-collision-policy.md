# MCP Registry Integration And Collision Policy

Status: Initial implementation for task 2939

Date: 2026-06-20

Depends on:

- `mcp-tool-discovery-and-schema-conversion`
- `per-agent-mcp-surface-architecture`
- `tool-architecture-registry-rules`

## Purpose

Discovered MCP tools must flow through the same canonical registry and inventory
model as local tools. Rusty Crew should not inherit an MCP-wins policy where a
server tool silently shadows an existing local capability.

## Implementation Location

Registry integration lives in:

`ts/packages/brain-island/src/mcp-tool-registry-integration.ts`

It exports:

- `integrateMcpToolsWithRegistry`
- `mcpCandidateToRegistryEntry`

`brain-island` depends on `adapter-mcp` for the candidate type only; discovery
still happens in the adapter.

## Integration Flow

The integration function:

1. starts with the base canonical registry entries;
2. converts MCP candidates into registry entries;
3. validates the combined registry with existing validation rules;
4. builds inventory through the existing `buildToolInventory` path;
5. returns a `tool_catalog_changed` payload for affected sessions/profiles.

This keeps selected, denied, missing, unavailable, and collision states visible
through the same inventory/diagnostic machinery already used by local tools.

## Collision Policy

Default policy is `fail`.

That means:

- MCP names colliding with local tool names fail registry validation;
- two MCP candidates with the same exposed name fail validation;
- MCP tools do not silently override local tools;
- inventory is only built when validation succeeds.

An explicit `prefix_source` policy can be used to rename MCP candidates that
collide with local names. This is deliberate and visible in the resulting
registry entry names.

## Unavailable Surfaces

Unavailable MCP tools are represented as inventory denials, not deprecations.
The integration layer maps them to `resource_denied` so sessions can explain
that a tool is expected by profile but unavailable due to surface/resource
state.

## Source Metadata

MCP registry entries retain source metadata:

- binding ID;
- adapter ID;
- server names;
- source tool name;
- endpoint ref;
- catalog revision;
- annotations;
- output schema.

This metadata is not part of the model-callable name, but it is available for
diagnostics and future execution routing.

## Covered Cases

`smoke:mcp-tool-registry` covers:

- merging local tools and MCP candidates;
- selecting MCP tools through `mcp:<tool_profile_key>` toolsets;
- `resource_denied` inventory for unavailable MCP tools;
- local-name collision failure by default;
- explicit source-prefix collision policy;
- duplicate MCP exposed-name collision failure;
- emitting a `tool_catalog_changed` payload.
