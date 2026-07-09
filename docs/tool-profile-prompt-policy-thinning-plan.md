# Tool, Profile, And Prompt Policy Thinning Plan

Status: implementation plan for Den task 4590
Date: 2026-07-07

## Purpose

The brain-island tool/profile/prompt cluster is intentionally TypeScript-heavy:
it talks to Node APIs, browser automation, web providers, Markdown/YAML profile
assets, pi-compatible tool wrappers, and prompt renderers. That is acceptable
as execution and presentation code. The problem is when those wrappers also
become durable policy owners for safety, selection, resource denial, telemetry
schema, context strategy, or delegated-session invariants.

This plan thins the TypeScript cluster around Rust-owned policy without moving
external client implementation into Rust for purity.

Use `tool-metadata-execution-authority-split-2026-07-09` for the precise
language around tool safety. In short: Rust owns durable metadata,
availability, resource facts, and policy planning; TypeScript still owns
selected-tool execution wrappers and adapter-local execution checks unless a
specific Rust planner has landed.

## Current Surfaces

Primary TypeScript files in scope:

- local code and patch tools: `local-code-tools.ts`, `patch-tool.ts`;
- web/browser tools: `web-tools.ts`, `browser-tools.ts`,
  `browser-session-manager.ts`;
- skills/MCP tool wrappers: `skills-tools.ts`, `mcp-brain-tools.ts`;
- profile loading and prompt assembly: `profile-loading.ts`,
  `profile-role-assembly.ts`, `delegated-role-assembly.ts`;
- context policy and estimates: `context-strategy.ts`,
  `context-estimate.ts`.

These files are about 6,400 lines together. They contain several distinct
concerns that should not be treated as one module.

## Keep In TypeScript

TypeScript remains the right home for:

- Node subprocess/file/browser/client execution;
- Playwright/CDP/browser lifecycle implementation;
- web search/extract provider calls and HTML/text extraction;
- SSRF checks that require Node networking details at fetch time;
- Markdown/YAML/profile asset loading and prompt text rendering;
- skill file parsing and display formatting;
- model-callable wrapper argument conversion;
- provider-specific token estimate approximations until exact estimators exist;
- user-facing command/tool text.

## Move Or Rust-Validate

Rust or Rust-validated artifacts should own:

- public tool metadata, toolset membership, safety flags, and denial reason
  vocabulary;
- local tool profile validation and selected tool inventory policy;
- session/profile/resource denial decisions that affect tool availability;
- durable tool-call telemetry schemas and event-kind vocabulary;
- browser/web session resource policy that is independent of JS client
  internals: max sessions, idle/lifetime caps, session scoping, cancellation,
  and archive/shutdown cleanup hooks;
- context strategy policy validation, threshold consistency, and durable
  compaction artifact/event contracts;
- branch-aware session memory selection and runtime-affecting profile defaults;
- delegated role invariants: parent/child ids, resource limits, allowed tools,
  and lifecycle links.

## Migration Slices

1. Thin local code and patch tools around Rust resource/tool policy. Keep file
   and subprocess execution in TS, but make resource limits, allowed write
   modes, telemetry result shapes, and denial codes Rust-validated.
2. Thin web/browser tools around Rust resource/session policy. Keep network,
   SSRF, browser automation, and snapshots in TS, but move session-scoping
   policy, browser resource caps, cleanup hooks, and safety metadata validation
   to Rust-owned or generated policy.
3. Move skills/MCP/local tool wrapper selection behind Rust-validated tool
   profile policy. Wrappers remain TS execution code; selected/denied inventory
   should come from the portable tool registry and local tool profile policy.
4. Split profile loading from runtime-affecting profile policy. TS can load
   JSON/YAML/Markdown and render prompts; Rust/core-config validates provider
   alias references, brain/session defaults, resource limits, MCP/channel
   defaults, context policy, and background jobs.
5. Move context strategy policy validation and durable event/artifact contracts
   toward Rust. TS can keep strategy rendering and provider estimates, but
   thresholds, debug visibility lanes, and compaction artifact contracts need a
   Rust/generated validation path.
6. Move delegated role invariants behind Rust lifecycle planning. TS can render
   delegated prompts, but parent/child identity, resource limit inheritance,
   selected tool profile, and lifecycle links should be Rust-owned.
7. Add classification and ratchet tests so remaining TS files are explicitly
   execution wrapper, prompt renderer, adapter glue, or temporary policy facade.

## Non-Goals

- Do not move Playwright/CDP/browser or web-provider clients into Rust now.
- Do not make Rust render soul/profile Markdown.
- Do not make tool wrappers model-facing unless selected through the
  Rust-validated tool policy.
- Do not preserve TS-only fallback policy once a Rust validator exists.
- Do not turn context strategy into a provider-specific brain implementation.

## Acceptance For The Series

- Tool wrappers do not carry durable safety/selection policy without Rust
  validation.
- Profile prompt assembly stays TS presentation, while runtime-affecting
  profile defaults and context policy use Rust/core-config or generated
  contracts.
- Delegated role assembly renders text but does not own lifecycle invariants.
- Remaining TS authority in this cluster is documented as intentional execution
  glue or temporary policy facade with follow-up tasks.
