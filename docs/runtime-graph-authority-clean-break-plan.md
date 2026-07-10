# Runtime Graph Authority Clean-Break Plan

Status: implementation contract; Rust normalization landed in #5373

Date: 2026-07-10

## Decision

`core-config` owns every deterministic decision that turns decoded service and
profile configuration into the effective runtime graph. TypeScript may decode
files, discover profile assets, construct provider and tool implementations,
and apply an accepted plan. It must not default, expand, repair, or independently
validate runtime-affecting graph records.

This supersedes the incremental posture in
`runtime-config-shape-duplication-reduction-plan.md`. The earlier parity smoke is
useful, but its loaded input has already had profile defaults, background-review
jobs, and MCP bindings applied by TypeScript. Passing that graph through Rust is
validation after the authoritative decisions have already happened.

There is no compatibility parser or inline fallback in the target design. A
missing service graph is created through the official initialization path; an
invalid graph is rejected with Rust reason codes.

## Input Boundary

TypeScript supplies three deliberately different inputs:

1. **Decoded runtime source**: JSON object values from `service.json`, without
   applying graph defaults.
2. **Reduced profile metadata**: runtime-only references projected from loaded
   profile assets. Prompt text, skill bodies, provider secrets, and tool
   implementations are excluded.
3. **Host facts**: resolved paths and environment facts that Rust cannot
   discover portably, such as the config directory, engine data directory,
   service default workdir, and whether a named secret environment variable is
   present. Facts are inputs, not policy decisions.

Rust deserializes the runtime source into canonical source DTOs and returns one
accepted `RuntimeGraphPlan`. The plan contains the complete effective graph,
derived-record provenance, stable diagnostics, and references that TS uses to
construct host capabilities.

## Canonical Plan Contents

The Rust plan must contain, in deterministic order:

- effective brain registrations, including canonical implementation ids;
- effective sessions, kinds, resource limits, history windows, owner ids,
  turn limits, local tool-profile references, context-policy references, and
  session-memory prompt references;
- authored and derived scheduled jobs, including background-review behavior
  and provenance;
- channel bindings with resolved session/profile/agent consistency;
- authored and profile-derived MCP bindings;
- service storage bootstrap plan and implementation status;
- service wake-timeout policy and per-session effective timeout source;
- TS-owned observation and MCP-server envelopes copied only as accepted adapter
  inputs, never used by Rust coordination;
- diagnostics with stable codes and source paths;
- a source fingerprint/revision suitable for reload and stale-plan rejection.

Tool implementations remain TS-owned. Rust plans the selected local tool
profile id and MCP tool-profile key so tool capability cannot silently differ
from the accepted graph.

## Function Inventory

The classifications below describe the endpoint, not current migration status.
An item marked **Rust** is not migrated merely because TS currently calls a
Rust validator afterward.

### Load, Normalize, And Preflight

| Current function | Target owner | Disposition |
| --- | --- | --- |
| `loadRustyCrewRuntimeConfig` | TS loader | Keep file I/O; pass decoded source and host facts to Rust; return the accepted plan view. |
| `preflightRustyCrewRuntimeConfig` | TS orchestration | Keep bridge/error-envelope orchestration; all diagnostics and summaries derive from the Rust plan. |
| `expandRuntimeConfigFromProfiles` | Rust | Delete; expansion is `core-config` authority. |
| `loadRuntimeProfiles` | TS profile adapter | Keep profile discovery and rich prompt/tool loading. |
| `loadRuntimeProfilesForValidation` | TS profile adapter | Keep loading; emit reduced generated profile metadata only. |
| `preflightFailure`, `preflightReport`, `errorMessage` | TS presentation | Keep stable API-envelope rendering; do not invent semantic diagnostics. |
| `sessionDefaultsApplied` | Rust diagnostics | Delete; Rust returns applied-default provenance. |
| `runtimeConfigValidationInputShape` | Generated boundary | Delete handwritten projection in favor of generated DTO conversion. |
| `assertRuntimeConfigPlan` | TS fail-closed adapter | Keep only accepted/rejected assertion over Rust diagnostics and source revision. |
| `backgroundReviewScheduledJob` | Rust | Delete. |
| `runtimeConfigFromNativeDraft` | Generated boundary | Delete handwritten reconstruction; consume generated plan DTOs. |
| `backgroundReviewPayloadForJob` | Rust plan plus TS executor | Rust owns review type and payload fields; TS host executor only invokes the named job implementation. |
| `emptyRuntimeConfig` | Delete | Missing-file fallback is removed; initialization must create an explicit source graph. |
| `validateRuntimeConfig` | Rust/generated boundary | Delete the graph parser/default engine. TS may parse JSON syntax only. |

### Runtime Effects And Host Capabilities

| Current function | Target owner | Disposition |
| --- | --- | --- |
| `applyRustyCrewRuntimeConfig` | TS effect adapter | Keep registration/application sequencing, but accept only a fingerprinted Rust plan. |
| `resolveModelProviderForBrain`, `modelProviderToBrainModelConfig`, `modelProviderApiKeySecret`, `modelProviderSecretEnvName`, `isRuntimeRecord` | TS provider adapter | Keep provider resolution and secret lookup. Rust plans provider/profile references, not SDK clients or secret values. |
| `rebuildConfiguredBrainRuntime` | TS effect adapter | Keep provider/tool reconstruction; selection and invalidation facts come from Rust. |
| `registerConfiguredScheduledJobs` | TS effect adapter | Keep host/session executor registration until the scheduler API absorbs it; execute only Rust-accepted jobs. |
| `createConfiguredBrain` | TS host composition | Keep provider and tool implementation construction from Rust references. |
| `createServiceToolResolver`, `createServiceTodoStore`, `createMemoryToolResolver`, `createServiceDenMemoryClient`, `serviceDenMemoryAvailability`, `denseProfileMemoryMode`, `createPlanningToolResolver` | TS host capabilities | Keep; these construct external or JS-side tool implementations. |
| `serviceSkillsDir`, `serviceSkillManageMode` | TS asset/tool adapter | Keep filesystem asset resolution and tool implementation mode; Rust owns any selected profile reference. |
| `toBridgeWakeExecutor`, `completionActionFromEvents`, `mergeTextDeltas`, `truncate` | TS host adapter | Keep provider/tool event adaptation; unrelated to graph authority. |

### Sessions, Defaults, And Runtime References

| Current function | Target owner | Disposition |
| --- | --- | --- |
| `sessionWithProfileDefaults`, `resourceLimitsWithDefaultWorkdir`, `effectiveSessionDefaults`, `definedDefaults` | Rust | Delete; effective values and provenance are planned by `core-config`. |
| `effectiveWakeTimeoutMs` | Rust plan | Delete precedence logic; TS timer consumes the effective duration or disabled state. |
| `nativeSessionConfig` | Generated boundary | Replace with generated plan-to-bridge conversion. |
| `configuredSessionForChannelBinding` | Rust | Delete; binding target resolution is a Rust graph query/plan. |
| `ensureConfiguredSessionForChannelBinding` | TS effect adapter | Keep ensure/reactivate effect using the Rust-resolved session plan. |
| `brainModuleDiagnostics` | TS projection | Keep display formatting only; module/strategy/provider-state decisions are Rust catalog outputs. |
| `channelBindingIdsForSession` | Rust plan/read model | Delete local graph search and consume resolved binding ids. |
| `effectiveModelMaxTokens` | Rust config plus TS provider limit | Rust owns profile/runtime precedence; TS may clamp to provider SDK constraints and report that clamp. |

### Service Envelope

| Current function | Target owner | Disposition |
| --- | --- | --- |
| `runtimeStorageConfig`, `runtimeStorageBackend`, `runtimePostgresBootMode`, `validateRuntimeStorageConfig` | Rust bootstrap plan | Move backend/default/identifier/status decisions into `core-config`; TS resolves paths and secret env values from host facts and opens the selected backend. |
| `runtimeWakeTimeoutConfig` | Rust plan | Move mode/default validation to Rust. |
| `runtimeDenObservationConfig`, `denObservationEventFilter` and their enum sets | TS adapter config | Keep as Den observation adapter input; it must not affect internal routing. Use a focused generated/checked adapter schema. |
| `configuredMcpServer` | TS MCP adapter | Keep URL/transport/client configuration. MCP servers are external capabilities, not coordination graph records. |

### Graph Record Parsers

| Current function | Target owner | Disposition |
| --- | --- | --- |
| `configuredBrain`, `configuredSession`, `resourceLimits`, `configuredScheduledJob`, `configuredChannelBinding`, `configuredMcpBinding`, `externalBindingStatus` | Rust source DTOs | Delete all handwritten parsing/defaulting. Rust serde plus `core-config` diagnostics own these records. |
| `configuredMcpServer` | TS MCP adapter | Retain in the narrow service envelope as noted above. |
| `arrayValue`, `optionalArrayValue`, `stringList`, `enumString`, `pathValue`, `requiredString`, `optionalString`, `optionalNumber`, `optionalBoolean`, `optionalPositiveInteger` | Generated schemas or narrow adapter parser | Remove from runtime graph parsing. Keep only helpers still required by Den-observation/MCP-server adapter config, preferably in those modules rather than this file. |
| `isAlreadyPresentError` | TS effect adapter | Keep idempotent bridge-effect recognition. |
| `isRecord`, `isNodeError` | TS loader | Keep generic syntax/I/O guards only. |

## Current Gaps The Migration Must Close

- The current valid parity fixture is already expanded by TS, so Rust reports
  no derived jobs or MCP bindings. Target fixtures start with authored input and
  require Rust to produce both derivations.
- `SessionConfigDraft` does not carry a local tool-profile reference, context
  policy reference, session-memory prompt reference, or effective wake-timeout
  provenance even though TS uses all four when constructing the runtime.
- storage aliases/defaults/status and wake-timeout precedence are still TS
  decisions.
- channel binding resolution is validated by Rust but re-selected locally by
  TS when channels create/ensure sessions.
- scheduled-job validation is split between TS cron parsing and Rust graph
  checks; one Rust diagnostic set must be canonical before effects run.
- `service.json` absence silently produces an empty graph. This hides bad
  deployments and is removed in the clean-break slice.

## Target Fixtures

`fixtures/runtime-config-parity/target/` is the pre-implementation contract:

- `complete-source.camel.json` contains decoded authored graph, reduced profile
  metadata, and host facts before defaults or derivation.
- `complete-plan.camel.json` contains the required effective graph and explicit
  derivation/default provenance.
- `invalid-source.camel.json` pins representative stable diagnostic codes for
  duplicate ids, broken references, invalid storage config, and invalid
  scheduled-job shape/targets.

`core-config::plan_runtime_graph` now consumes these fixtures directly in Rust
tests and produces the valid plan and required diagnostics. This proves Rust
normalization, not bridge/TypeScript adoption. #5377 makes the DTO coverage
generated or generated-checked, and #5381 removes the superseded TS path.

## Migration Sequence

1. **#5373: Rust plan.** Add canonical source/host-fact/plan DTOs, normalize the
   complete fixture in Rust, emit stable provenance and diagnostics, and remove
   semantic dependence on pre-expanded TS input.
2. **#5377: Generated boundary.** Generate or generated-check TS DTOs, raw
   bridge mappings, schemas, field inventories, and fixture drift gates.
3. **#5381: TS deletion.** Replace the graph parser with file decoding plus one
   Rust planning call, delete the functions marked for removal, require an
   explicit initialized graph, and add a boundary/size ratchet.
4. **#5385: live certification.** Exercise create/reload/rebuild through APIs on
   the SQLite debug service and isolated PostgreSQL test profiles. Verify plan
   fingerprint readback and that rejected/stale config never remains active.

## Required Gates

Each slice runs focused Rust/config and parity checks. The deletion slice runs
the full offline gate. Live certification is separate evidence and must not be
replaced by deterministic fixtures.

```sh
cargo test -p rusty-crew-core-config
cargo clippy --workspace --all-targets -- -D warnings
npm run smoke:runtime-config-parity
npm run smoke:core-config-facade-drift
npm run smoke:bridge-contract-parity
npm run smoke:bridge-validation
npm run typecheck
```
