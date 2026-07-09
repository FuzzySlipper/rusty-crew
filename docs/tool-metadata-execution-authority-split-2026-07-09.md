# Tool Metadata And Execution Authority Split

Status: implementation note for task 5304
Date: 2026-07-09

## Purpose

This note clarifies what Rusty Crew means when it says Rust owns tool policy or
tool safety. Recent migration work moved durable metadata and availability
decisions toward Rust, but ordinary tool execution still happens in TypeScript.

The distinction matters because "tool safety" can mean two different things:

1. Whether a tool should exist in a profile/session catalog at all.
2. Whether one concrete tool call is safe to execute against a JS, filesystem,
   browser, web, MCP, or adapter client.

Rust owns the first category. TypeScript often still owns the second category
when the executor is a TypeScript client wrapper.

## Rust-Owned Authority

Rust or Rust-validated artifacts own durable tool policy:

- canonical tool metadata and public descriptors;
- tool names, aliases, categories, toolsets, surfaces, versions, deprecation,
  replacement, and output-shape metadata;
- portable safety flags as catalog facts;
- collision and duplicate-capability diagnostics;
- profile/toolset selection and selected inventory shape;
- profile/session/resource denial reason vocabulary where availability is
  affected;
- local tool profile validation;
- MCP normalized metadata validation before exposure;
- durable telemetry/event schema when tool calls become service records.

Rust-owned policy answers: "Should this session see this tool, and how should
the service explain that decision?"

## TypeScript-Owned Execution Glue

TypeScript may own executor-local safety checks when the executor is a JS,
Node, browser, web, MCP, or adapter client:

- filesystem path resolution and subprocess invocation in `local-code-tools.ts`;
- patch application mechanics in `patch-tool.ts`;
- redirect, port, DNS, and private-network checks in `web-tools.ts`;
- browser/CDP interaction and browser ref handling in `browser-tools.ts`;
- Chromium process/session management in `browser-session-manager.ts`;
- skill file parsing and display/mutation wrapper behavior in `skills-tools.ts`;
- MCP optional-argument pruning and client invocation in `mcp-brain-tools.ts`.

These checks answer: "Given a selected tool and already-frozen session facts,
can this specific adapter/client operation run?"

## Boundary Rule

Do not describe a task as "Rust validates tool safety" unless the landed Rust
code validates the relevant behavior.

Preferred wording:

- "Rust validates portable tool metadata."
- "Rust plans/validates tool availability for profile/session selection."
- "TypeScript executes the selected tool and performs adapter-local execution
  checks."
- "A future Rust planner should own this resource or lifecycle decision."

## Follow-Up Candidates

The current split is acceptable where TypeScript is only an executor wrapper.
Move more authority behind Rust planners when a TypeScript check becomes one of
these:

- a reusable profile/session resource decision;
- a lifecycle ceiling or cleanup guarantee;
- a durable mutation gate;
- a cross-tool or cross-adapter denial vocabulary;
- a service-visible telemetry or audit contract.

Concrete candidates for future implementation tasks:

- browser/web resource caps and lifecycle cleanup planning;
- local code and patch write-mode/resource-limit planning;
- skill mutation governance through durable curator storage.

The skill mutation item overlaps with #5305. The local-code and browser/web
items should become implementation tasks when those surfaces next need runtime
behavior changes.

## Validation

The current metadata/registry validation path remains:

```bash
npm run smoke:tool-registry-parity -w @rusty-crew/brain-island
npm run smoke:tool-registry-diagnostics -w @rusty-crew/brain-island
npm run smoke:local-tool-profile-policy -w @rusty-crew/brain-island
cargo test -p rusty-crew-core-tool-registry
```
