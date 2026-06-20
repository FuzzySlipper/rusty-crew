# MCP Tool Discovery And Schema Conversion

Status: Initial implementation for task 2938

Date: 2026-06-20

Depends on:

- `mcp-client-transports-and-lifecycle`
- `per-agent-mcp-surface-architecture`
- `tool-architecture-registry-rules`

## Purpose

MCP tools should be discovered per surface and converted into registry-ready
tool candidates without being globally exposed. Selection, collision policy,
profile filtering, and registry insertion remain separate follow-up work.

## Implementation Location

Discovery and conversion live in:

`ts/packages/adapter-mcp/src/mcp-discovery.ts`

The module exports:

- `discoverMcpToolCandidates`
- `convertMcpToolsToCandidates`
- `normalizeMcpInputSchema`
- `createMcpPiAgentTool`

## Discovery Shape

The discovery client interface is intentionally small:

`listTools(): McpDiscoveredTool[]`

Real MCP clients from the lifecycle layer can implement this after connection.
The discovered tool record includes:

- source tool name;
- description/title;
- input JSON Schema;
- output JSON Schema;
- annotations.

## Candidate Shape

Each discovered tool converts to an `McpRegistryCandidate` with:

- model-callable name;
- description;
- `category: "mcp"`;
- `toolsets: ["mcp:<tool_profile_key>"]`;
- implementation module marker;
- safety flags from annotations;
- output shape;
- source identity metadata;
- typebox-compatible `parameters`;
- original output schema and annotations.

Source identity always includes binding ID, adapter ID, server names, source
tool name, endpoint ref, and catalog revision. Endpoint refs are safe config
references, not raw credentials.

## Schema Conversion

MCP input schemas are sanitized into typebox-compatible parameter schemas. The
converter preserves common JSON Schema fields such as:

- type;
- properties;
- required;
- items;
- additionalProperties;
- enum/const/default;
- string and number bounds;
- anyOf/oneOf/allOf;
- `$defs` and `definitions`.

Non-object root schemas are wrapped as `{ value: ... }`, because pi-compatible
agent tools expect object-style parameter payloads. Boolean `true` schemas
become empty objects. Boolean `false` schemas are sanitized with a warning.
Nullable type arrays are converted to a non-null type plus `nullable: true`.

## Pi-Compatible Tool Shape

`createMcpPiAgentTool` returns the same runtime shape used by current
`@earendil-works/pi-agent-core` tools:

- `name`;
- `description`;
- `label`;
- `parameters`;
- `executionMode`;
- `execute(...)`.

The adapter uses a local structural mirror rather than importing the upstream
type directly. This avoids pulling unrelated provider SDK type dependencies
into `adapter-mcp` while preserving runtime compatibility with the pi tool
shape.

## Deferred To Later Tasks

This task does not:

- insert MCP candidates into the canonical registry;
- decide naming collisions;
- select tools for a profile/session;
- execute real MCP protocol calls;
- persist discovered catalog revisions.

Those are owned by tasks `2939`, `2940`, and `2941`.

## Covered Cases

`smoke:mcp` now covers:

- converting discovered tools into per-binding candidates;
- preserving source identity and tool profile key;
- schema wrapping for non-object schemas;
- sanitation warnings for false schemas;
- duplicate source tool diagnostics;
- destructive annotation mapping to `external_write`;
- pi-compatible tool execution through a surface executor.
