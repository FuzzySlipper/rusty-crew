# MCP Per-Agent Surfaces E2E Proof

MCP is a per-agent/profile tool surface in Rusty Crew. It is not a centralized global tool pool. Each hosted agent can have its own binding, server names, tool-profile key, discovered revision, registry selection, telemetry, and reload lifecycle.

## Scenario

`smoke:mcp-surfaces-e2e` proves:

- two Rust sessions exist for two agents;
- two MCP bindings connect as separate surfaces;
- each profile discovers a different MCP tool set;
- registry selection uses the matching `mcp:<toolProfileKey>` toolset;
- each agent receives only its selected MCP tools;
- the Pi-compatible MCP tool wrapper executes against the correct binding and source tool;
- local/MCP name collisions fail closed by default;
- explicit namespacing can resolve an intentional collision;
- cross-profile tool-profile checks deny the wrong MCP tool;
- Rust receives MCP catalog-change events;
- Rust receives MCP tool start/finish telemetry with the correct session/profile metadata;
- reloading one MCP surface does not disturb the other active surface.

## Guardrails

The proof keeps alpha and beta surfaces distinct from the binding record through discovery, registry integration, execution, telemetry, and reload. This is the critical behavior that prevents MCP from drifting into the older centralized adapter pattern.

Tool execution telemetry remains payload-free. Rust stores tool call metadata and catalog-change facts; the TypeScript MCP adapter owns transport and tool invocation details.

## Verification Command

Run:

```bash
npm run smoke:mcp-surfaces-e2e
```

Expected output includes:

- `alphaTools: ["alpha_search"]`;
- `betaTools: ["beta_summarize"]`;
- two calls bound to `mcp-alpha` and `mcp-beta`;
- `toolTelemetryRows: 4`;
- `betaStatusAfterAlphaReload: "active"`;
- `collisionBlocked: true`.
