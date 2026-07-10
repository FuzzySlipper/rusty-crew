# TypeScript Fallback And Legacy Path Audit

Status: implementation note for task 5307, amended by task 5389
Date: 2026-07-09

## Purpose

This audit checks whether the Rust authority migration left TypeScript fallback,
legacy, shim, scaffold, or deterministic paths that could quietly become new
production development lanes.

The goal is not to ban ordinary fallback values or compatibility parsing inside
well-named production modules. The risk is production files or package exports
whose identity implies old behavior is still a supported alternate path after
Rust became the authority for coordination, storage policy, and brain runtime
selection.

## Current Findings

| Surface | Classification | Disposition |
| --- | --- | --- |
| `ts/packages/brain-island/src/legacy-pi-agent-test-harness.ts` | Smoke-only legacy test harness | Moved to `ts/packages/brain-island/smokes/support/legacy-pi-agent-test-harness.ts`. Production `src` no longer owns this path. |
| `ts/packages/brain-island/src/legacy-pi-tool-adapter-test-harness.ts` | Smoke-only legacy test harness | Moved to `ts/packages/brain-island/smokes/support/legacy-pi-tool-adapter-test-harness.ts`. Existing src smokes temporarily import smoke support until #5303 moves all smokes out of `src`. |
| `capture-memory-proposals.ts` legacy dense proposal conversion | Internal compatibility parser for older dense-memory proposal shape | Kept internal because `background-memory-skill-review.ts` still accepts the old shape. Removed from `package-surface/memory.ts` so the public package surface does not invite new callers. Future memory work can remove the legacy parser when producer outputs are typed-only. |
| `native-bridge/src/index.ts` `unavailable(...)` proxy | Fail-closed native-loader placeholder for missing bridge methods | Kept. This is not a behavior fallback because every method rejects with a missing-native error. It exists so the TS type surface can be constructed before a native binding is loaded. |
| former `localBrainModule` | Explicit local deterministic brain module | Deleted from production by #5389. Rust rejects `local` as a module id; deterministic executors live only in `smokes/support`. |
| `createPlaceholderBrain` | Former deterministic test alias | Exists only in `smokes/support/local-brain-test-support.ts` and is explicitly asserted absent from the package root. |

## Guardrail Added

`tools/check-ts-package-boundaries.mjs` now checks production
`@rusty-crew/brain-island` filenames for residue terms:

- `legacy`
- `fallback`
- `compat`
- `shim`
- `scaffold`
- `placeholder`
- `deterministic`

Files matching those terms under production `src` must be moved to
`smokes/`/test support, renamed to their durable role, or added to an explicit
reviewed allowlist with a Den task.

This is intentionally filename-based. Normal parsing helpers that use fallback
values are not blocked unless the whole file presents itself as an old alternate
implementation.

## Relationship To #5303

This task does not finish the smoke relocation. Many smoke files still live
under `ts/packages/brain-island/src`. The moved legacy harnesses are now under
`smokes/support`, and those legacy src smokes temporarily import them from
there.

#5303 remains responsible for the thorough cleanup target:

```bash
rg --files ts/packages/brain-island/src | rg '(^|/)smoke.*\.ts$'
```

must eventually return no files.

## Follow-Up

- Remove the legacy dense proposal parser once capture producers and background
  review outputs are typed-only.
- Retire `createPlaceholderBrain` from smoke support when older smoke wording no
  longer benefits from the alias; it is not a production export.
- As part of #5303, move remaining smokes out of production `src` so smoke
  support imports no longer cross from `src` to `smokes`.
