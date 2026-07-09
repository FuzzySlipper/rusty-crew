# Architecture Hygiene Triage 2026-07-07

Status: task triage note for #4514

This note records the July 6 residual architecture hygiene triage. The goal was
to keep smaller review findings from disappearing without turning the grab bag
into broad refactor work.

## Fixed In #4514

Several historical docs and one live smoke were mode `0600`, which can surprise
fresh local agents and non-owner service users. These files were normalized to
normal source/document permissions (`0644`):

- `docs/pi-agent-rust-port-inspiration.md`
- `docs/pi-crew-upstream-audit.md`
- `docs/README.md`
- `docs/pi-crew-extraction-analysis.md`
- `docs/pi-crew-core-bridge-manifest.md`
- `ts/packages/brain-island/smokes/smoke-roleplay-quality-spike-live.ts`

## Split To Follow-Up Tasks

- #4642: retire or revive unused `core-bridge-mock`.
  Triage confirmed it has no workspace consumers beyond Cargo membership,
  governance metadata, and docs references.
- #4643: decide native bridge binary tracking strategy.
  Triage confirmed the generated `.node` artifact remains tracked at roughly
  18 MB and local builds can leave it dirty.
- #4644: add Den successor Gateway API version negotiation or path-prefix
  boundary.
  Triage confirmed `/v1` paths are scattered through the successor Gateway
  adapter.
- #4645: review buffered brain run registry ownership outside bridge globals.
  Triage confirmed OpenAI Responses and PI agent buffered runs use bridge-crate
  process-global registries.
- #4646: reduce service runtime config shape duplication after parity guard.
  Triage confirmed `service-runtime-config.ts`, native bridge raw/generated
  types, and `core-config` still duplicate shape authority even though
  `smoke:runtime-config-parity` catches drift.

## Accepted For Now

The existing runtime config parity guard remains useful and should stay in
`verify:ts`; #4646 is about reducing duplicated authority, not removing the
guard before replacement.

The tracked native bridge binary was not changed in #4514. A local build had
already modified it in the worktree, but that artifact churn is intentionally
left for #4643 rather than mixed into the triage commit.
