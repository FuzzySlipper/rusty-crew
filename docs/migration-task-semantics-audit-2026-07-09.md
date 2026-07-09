# Migration Task Semantics Audit

Status: audit note for task 5306
Date: 2026-07-09

## Purpose

Recent Rust-authority work used several useful task shapes: some moved
behavior into Rust, some added CI ratchets, some documented an intentionally
TypeScript-owned boundary, and some only named a future migration path.

Those are all valid outcomes, but they must not be read as the same thing.
This note is the vocabulary to use when reading completed migration tasks and
architecture docs.

## Classification Vocabulary

Use these buckets in task packets, review notes, and follow-up docs.

| Bucket | Meaning | Do not imply |
| --- | --- | --- |
| `migrated` | Runtime behavior, durable policy, storage semantics, or deterministic planning moved behind Rust code or a Rust-owned generated/validated artifact, and the production TS path now calls that boundary. | That every adjacent route, debug projection, or executor wrapper was also moved. |
| `ratcheted` | CI, smoke, fixture, or boundary checks now prevent new drift or make drift visible. | That the old surface has been decomposed or fully relocated. |
| `certified-current-boundary` | A TS surface was inventoried and declared acceptable for now because it is route glue, adapter/client glue, prompt rendering, or executor binding. | That the surface became Rust-owned. |
| `planned` | A doc or task names the target Rust/codegen boundary and slices. | That any implementation landed. |
| `residual-deferred` | A known scaffold, large file, legacy location, or TS-owned policy remains and is explicitly tracked. | That the project accepts the shape permanently. |

The shortest rule: a task is only `migrated` when production code now depends on
the Rust/codegen boundary for the decision being claimed.

## Reviewed Set

This audit reviewed the current residue-cleanup parent and the related docs
that are most likely to be misread as complete migrations:

- #5299 through #5307, plus #5308 and #5309 created from #5304.
- `docs/typescript-authority-refactor-catalog-2026-07-07.md`
- `docs/native-bridge-rust-contract-mapping-migration.md`
- `docs/typescript-memory-surface-inventory-2026-07-08.md`
- `docs/typescript-tool-profile-prompt-surface-inventory-2026-07-08.md`
- `docs/tool-registry-rust-authority-migration.md`
- `docs/tool-metadata-execution-authority-split-2026-07-09.md`
- `docs/engine-store-boundary-migration-plan.md`
- `docs/smoke-test-inventory.md`
- `docs/curator-mutation-executor-safeguards.md`
- `docs/roleplay-boundary-and-rust-migration-plan.md`

## Current Bucket Map

### Migrated

These docs contain real migration claims, but only for the named invariant:

- `tool-registry-rust-authority-migration.md`: portable built-in tool metadata
  is Rust-validated through `core-tool-registry`, local tool profile validation
  calls `validate_local_tool_profile_policy`, and dynamic MCP metadata passes
  through `validate_tool_metadata_policy`.
- `typescript-memory-surface-inventory-2026-07-08.md`: profile/session memory
  storage, memory proposal validation, governance decisions, and several
  curator transition planners are Rust/native operations.
- `roleplay-boundary-and-rust-migration-plan.md`: listed roleplay invariants
  such as assistant alternative planning, speaker identity, narrator config
  normalization, and roleplay session/control planners have Rust coverage when
  named in the certification matrix.

### Ratcheted

These are guardrail wins, not full relocation:

- `native-bridge-rust-contract-mapping-migration.md`: bridge operation parity,
  fixture drift, fingerprint drift, and validation coverage checks exist, but
  raw TS mappings and many TypeBox schemas are still hand-maintained.
- `smoke-test-inventory.md`: new smoke placement is constrained and lanes are
  classified, but many old `brain-island/src/smoke-*.ts` files remain until
  #5303 finishes the relocation.
- #5307: production filenames with residue terms are blocked unless explicitly
  allowlisted, and legacy test harnesses were quarantined under smoke support.
  This reduces accidental fallback growth; it is not a complete proof that no
  stale branch remains anywhere.

### Certified Current Boundary

These docs are intentionally classification-only unless they name a Rust/native
operation in their tables:

- `typescript-tool-profile-prompt-surface-inventory-2026-07-08.md`: local code,
  patch, web, browser, skills, MCP, profile loading, prompt rendering, context
  estimation, and delegated prompt assembly surfaces are classified. The doc
  allows TS adapter/executor/prompt glue under Rust-planned facts; it does not
  claim those executors moved to Rust.
- `typescript-memory-surface-inventory-2026-07-08.md`: Den memory tools,
  memory-space admin wrappers, dense/profile memory tools, lore tool wrappers,
  and route glue can remain TS wrappers where they call Rust/native storage or
  external adapters. The wrapper classification is not itself migration.
- `tool-metadata-execution-authority-split-2026-07-09.md`: Rust owns portable
  metadata and availability policy; TypeScript still owns selected tool
  execution and adapter-local checks.

### Planned

These docs should be read as roadmaps:

- `engine-store-boundary-migration-plan.md`: `CoreEngine` still reaches a wide
  `CoreCoordinationStore` surface. #5300 owns the actual domain store port
  extraction.
- `native-bridge-rust-contract-mapping-migration.md`: #5302 owns shrinking or
  generating/checking raw TS mapping surfaces beyond the existing ratchets.
- `typescript-authority-refactor-catalog-2026-07-07.md`: the catalog is a
  prioritization inventory. A section title there does not mean the migration
  has landed.
- #5308 and #5309: browser/web resource caps and local code/patch resource
  facts are follow-up implementation tasks from #5304.

### Residual Deferred

These are real known residues:

- `service-app.ts` remains a central composition and route-dispatch file. #5301
  owns decomposing it without moving authority in a sloppy way.
- `native-bridge/src/index.ts` still contains large handwritten mapping
  surfaces. #5302 owns the codegen/checking work.
- `brain-island/src` still contains legacy smoke files. #5303 owns moving the
  count to zero.
- `MemoryCuratorGovernanceStore` is still identified as a smoke/test scaffold
  in `curator-mutation-executor-safeguards.md`. #5305 owns durable governance
  storage.

## Wording Rules

Use precise verbs in future task titles and packets:

- Say `migrate`, `move`, or `Rust owns` only when the production behavior now
  goes through Rust or a Rust-owned generated/validated artifact.
- Say `ratchet`, `guard`, or `validate coverage` when the work prevents drift
  but leaves the old implementation surface in place.
- Say `inventory`, `classify`, or `certify boundary` when the work documents
  why TS remains acceptable.
- Say `plan` when the output is a roadmap.
- Say `quarantine` when a residue is isolated so it cannot silently attract new
  production development.

Misleading example:

```text
Validate local code tool safety in Rust
```

Preferred if only metadata moved:

```text
Validate local code tool metadata in Rust and plan execution resource facts
```

Preferred if the executor still lives in TS:

```text
Classify local code execution wrapper boundary
```

## Follow-Up Ownership

This audit does not close implementation gaps by naming them. The active
follow-up tasks own the actual work:

- #5300: engine domain store ports.
- #5301: `service-app.ts` decomposition.
- #5302: native bridge raw mapping/codegen cleanup.
- #5303: move all brain-island smokes out of production `src`.
- #5305: curator durable governance storage.
- #5307: stale TS fallback and legacy path cleanup.
- #5308: Rust-owned browser and web resource caps.
- #5309: Rust-owned local code and patch resource facts.

When one of those tasks lands, its packet should name the bucket it achieved
and the exact production decision that moved or was ratcheted.
