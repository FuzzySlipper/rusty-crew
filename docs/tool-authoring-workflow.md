# Tool Authoring Workflow

Status: Accepted project reference for task 2863

Date: 2026-06-20

## Purpose

Rusty Crew tools must be easy to extend without creating drift. A future agent
should not add `do_thing2` because it missed an existing `do_thing`, and it
should not bypass Rust-owned session/resource contracts when adding a
TypeScript executor.

Read this with `docs/tool-architecture-registry-rules.md`. That document owns
the architecture. This document is the day-to-day workflow.

## Where Tools Live

Model-callable tools are TypeScript brain-island code.

- Registry metadata lives in
  `ts/packages/brain-island/src/tool-registry.ts`.
- Registry diagnostics live in
  `ts/packages/brain-island/src/tool-registry-diagnostics.ts`.
- Profile/session selection lives in
  `ts/packages/brain-island/src/tool-profile-selection.ts` and
  `ts/packages/brain-island/src/tool-session-selection.ts`.
- Local code implementations live in
  `ts/packages/brain-island/src/local-code-tools.ts`.
- The `patch` implementation lives in
  `ts/packages/brain-island/src/patch-tool.ts`.
- Production wake proofs live in `ts/packages/brain-island/src/smoke-*.ts`.

Do not construct ad hoc per-agent tool lists. Profiles request toolsets or
explicit tool names; final model-callable tools must flow through the registry,
ToolProfile selection, and session filtering path.

## Adding A Tool

1. Search first:
   - `rg -n "tool_name|output_shape|category" ts/packages/brain-island/src`
   - Run or inspect `npm run smoke:tool-registry-diagnostics`.
2. Add or update one canonical registry entry in `tool-registry.ts`.
3. Include required metadata:
   - canonical lower-snake-case `name`
   - `description`
   - `category`
   - `toolsets`
   - `implementationModule`
   - `surfaces`
   - `safety`
   - `outputShape`
   - `version`
   - `inventoryTest`
4. Add the implementation in the correct TS module.
5. Wire the implementation into a resolver such as `resolveLocalCodeTools`.
6. Add a focused smoke proving the tool can be selected and invoked.
7. Add or extend a production wake proof if the tool changes bridge behavior,
   resource enforcement, or durable telemetry.
8. Regenerate the shared portable metadata artifact when registry metadata
   changes:
   - `npm run generate:tool-registry-artifact`
9. Run:
   - `npm run format`
   - `npm run typecheck`
   - `npm run smoke:tool-registry-parity`
   - relevant `npm run smoke:*`
   - `cargo test -p rusty-crew-core-tool-registry`
   - `cargo clippy --all-targets --all-features -- -D warnings` when Rust
     contracts, bridge shapes, persistence, or telemetry changed.

## Naming Rules

Use one canonical name. Prefer verb-object names such as `read_file`,
`search_files`, `git_diff`, or `request_delegation`.

Avoid numbered variants. If behavior must change, either:

- update the existing implementation without changing its contract;
- add a new canonical tool and deprecate the old one; or
- add an explicit coexistence note if two tools intentionally share a category
  and output shape.

Aliases are compatibility metadata, not independent tools.

## Deprecating Or Renaming A Tool

Renames are registry migrations.

1. Add the replacement entry.
2. Mark the old entry as deprecated with a replacement or sunset note.
3. Keep an alias only when compatibility is needed.
4. Update diagnostics and smokes so the old and new names cannot silently both
   appear as active tools.
5. Migrate profile configs and any persisted references.
6. Remove the alias only after old references are gone.

Deprecated tools should be denied or explained in inventory output unless a
profile explicitly opts into deprecated tools.

## When Rust Must Change

Most ordinary tool implementation stays TypeScript-side. Add Rust contract or
hook work when a tool:

- changes session lifecycle, delegation, completion, wake scheduling, or
  cancellation behavior;
- needs durable telemetry beyond generic tool start/end events;
- introduces a new resource limit or enforcement contract;
- changes `ToolProfile`, `BrainEvent`, `BrainAction`, or native bridge shapes;
- needs restart-safe audit or replay behavior.

Current durable tool execution telemetry is the Rust `tool_call_history`
projection populated from `tool_call_started` and `tool_call_finished` brain
events. If a new tool needs richer audit fields, add a typed contract rather
than hiding data in free-form text output.

## Adapting Pi-Crew Tools

Pi-crew tools are useful sources, but port by responsibility:

- Keep model-callable execution in TypeScript.
- Keep Rust-owned coordination as `BrainAction`s and `BrainEvent`s.
- Remove worker-pool assumptions unless the feature is actually worker-pool
  specific.
- Replace Hermes/profile-specific path guards with Rusty Crew session
  resource limits.
- Preserve useful safety behavior such as bounded workdirs, syntax rollback,
  stable details objects, and unified diff output.

The first adapted examples are `local-code-tools.ts` and `patch-tool.ts`.

## Proof Expectations

Narrow tools can start with a focused smoke. Shared or high-risk tools need a
production bridge proof.

Existing proof scripts:

- `npm run smoke:tool-registry`
- `npm run smoke:tool-registry-parity`
- `npm run smoke:tool-registry-diagnostics`
- `npm run smoke:tool-profile-selection`
- `npm run smoke:tool-session-selection`
- `npm run smoke:local-code-tools`
- `npm run smoke:patch-tool`
- `npm run smoke:production-local-tools-wake`
- `npm run smoke:production-patch-wake`

If a stub or fake is intentionally left behind, create a child task for the
parent feature before closing the task that introduced it.
