# Per-Agent MCP Surface Architecture

Status: Design for task 2936

Date: 2026-06-20

Depends on:

- `tool-architecture-registry-rules`
- `tool-authoring-workflow`
- `channel-mcp-binding-persistence`
- `multi-agent-adapter-architecture`

## Purpose

Rusty Crew should support MCP tools without recreating pi-crew's old centralized
client problem. MCP remains TypeScript-side for now, but Rust owns the runtime
identity, session, profile, resource, and audit contracts that decide which
tools a brain may see.

Each hosted agent can have its own MCP surface. A surface is the selected set of
MCP servers, discovered tools, profile string, diagnostics, and lifecycle state
for one runtime identity scope.

## Ownership Boundary

TypeScript owns:

- MCP client transports and process/http lifecycle;
- server selection from safe config references;
- tool discovery and JSON Schema conversion;
- model-callable tool implementation wrappers;
- reload and per-surface diagnostics;
- conversion into canonical registry/inventory candidates.

Rust owns:

- durable agent/session/profile identity;
- `mcp_bindings` records and query scopes;
- `ToolProfile` descriptors attached to sessions;
- resource limits and session filtering;
- durable telemetry for tool start/end/error through brain events or future
  typed tool events;
- lifecycle authority for wake, cancellation, drain, timeout, and restart.

MCP clients must not become a coordination bus. They expose tools to the brain
island; they do not own session routing, delegation, completion, or wake policy.

## Surface Scope

An MCP surface is keyed by a durable MCP binding:

- `binding_id`;
- `adapter_id`;
- `agent_id`;
- optional `instance_id`;
- optional `session_id`;
- `profile_id`;
- `server_names_json`;
- `endpoint_ref`;
- `transport`;
- `tool_profile_key`;
- `discovered_tool_revision`;
- `status` and safe diagnostics.

There is no global "current MCP client." Two agents in the same Rusty Crew
service may use different server sets, different tool profile strings, and
different discovered tool revisions at the same time.

## Selection Flow

The intended flow is:

1. Rust/session/profile selection identifies the MCP binding scope.
2. TypeScript MCP adapter resolves safe config references into live clients.
3. Adapter discovers server tools and converts schemas into registry candidates.
4. Candidates feed the canonical tool registry/inventory layer.
5. Profile/toolset/session/resource filtering selects final model-callable
   tools.
6. Final descriptors are mirrored into Rust `ToolProfile` for session audit.
7. Brain execution calls TS MCP tool wrappers selected for that session.

The brain must not receive tools by directly concatenating per-server lists.
All MCP tools pass through the same registry diagnostics, collision rules,
deprecation handling, and inventory explanation as local tools.

## Tool Identity And Collisions

Every imported MCP tool needs stable source identity metadata:

- server name;
- binding ID;
- source tool name;
- source schema revision or catalog revision;
- exposed model-callable name;
- output shape/schema family;
- collision/deprecation status.

Model-callable names should prefer the server's natural name only when it is not
ambiguous. If two surfaces expose overlapping names or output shapes, registry
diagnostics must report the collision before selection. Prefixing is a selection
policy choice, not an ad hoc workaround inside the executor.

## Reload And Diagnostics

Reload is per surface. A reload should update:

- discovered tool revision;
- selected server list;
- tool inventory outcomes;
- degraded reason, if discovery or transport fails;
- safe diagnostic summaries.

Reloading one agent/session/profile surface must not mutate another surface's
selected tools. Diagnostics should answer "why does this session have this MCP
tool?" and "why is that expected tool missing?" without exposing secrets.

## Secret Boundary

Durable records and diagnostics may store:

- endpoint refs such as `config://mcp/project-main`;
- server names;
- transport labels;
- catalog revisions;
- bounded error summaries;
- selected/denied/collision inventory outcomes.

They must not store:

- raw tokens or auth headers;
- private keys;
- complete provider config blobs;
- unbounded tool input/output payloads;
- raw server responses that may contain secrets.

## Implementation Sequence

The following child tasks should build on this model:

- `2937`: port MCP client transports and lifecycle manager as per-surface TS
  clients.
- `2938`: port discovery and JSON Schema conversion into registry candidate
  generation.
- `2939`: integrate candidates with the canonical registry and collision
  policy.
- `2940`: implement per-session reload and diagnostics.
- `2941`: add Rust-side resource hooks and tool execution telemetry where MCP
  tools need durable audit or enforcement.
- `2946`: prove two agents can hold independent MCP surfaces end to end.

## Non-Goals

This design does not require Rust to execute MCP calls. It also does not require
building worker-pool policy into MCP. Worker-pool agents, full agents, prime
agents, and delegated agents should all receive MCP tools through the same
profile/session selection path.
