# ADR 0019: DB-Backed Profile Registry And File Assets

Status: Proposed for task 3375

Date: 2026-06-25

## Context

Rusty Crew currently loads profiles from files. The service supports flat JSON
profiles and profile directories with `profile.yaml`, `soul.md`, and
`memory.md`. Runtime service config also contains profile-related graph entries
such as brains, sessions, channel bindings, MCP bindings, and scheduled jobs.

This worked well for bootstrapping because files are visible, editable,
git-friendly, and easy to compare with pi-crew/Hermes examples. It is becoming
awkward for GUI-created profiles and runtime admin surfaces: creating a profile
means writing profile files, mutating `service.json`, reloading config, and
knowing which derived entries must be present.

Rusty View and future frontends need an official profile create/edit path that
does not require file choreography. At the same time, humans still need to
inspect and export prompts, skills, and profile templates.

## Decision

Rusty Crew will move toward a DB-backed active profile registry while keeping
file-backed profile assets.

The DB-backed registry is authoritative for active runtime profile state. File
assets remain authoritative for human-authored prompt material and reusable
templates unless explicitly imported into the registry.

The service must provide import/export paths in both directions:

- import file-backed starter profiles into the DB registry;
- export DB-backed profiles and prompt assets to files for backup, review, and
  portability.

This is a migration path, not a big-bang replacement. Existing file-backed
profiles remain loadable while the registry lands.

## Current Responsibilities

### Profile Files

Current file-backed profile config owns or implies:

- `profileId`, `displayName`, and identity-like metadata;
- model/provider config;
- brain module/strategy selection;
- runtime turn/resource defaults;
- tool policy;
- prompt fragments;
- `soul.md` and `memory.md` static prompt text;
- skill selection and profile skill roots;
- MCP config defaults;
- background review config;
- memory config;
- session defaults;
- channel defaults.

### Service Runtime Config

`service.json` owns the active runtime graph:

- brain registrations;
- sessions;
- scheduled jobs;
- channel bindings;
- MCP bindings;
- global `profilesDir` and `skillsDir`.

### Runtime Persistence

Rust persistence owns sessions, runtime events, queues, dense profile memory,
provider state, counters, tool telemetry, and other runtime facts.

## Target Split

### DB-Authoritative Active Registry

The DB-backed profile registry should own active service/runtime state:

- profile id;
- lifecycle status: active, paused, decommissioned, archived;
- display name and operator-facing summary;
- profile kind/default session kind;
- agent/member identity defaults;
- owner id/session defaults;
- model/provider config selected through UI/API;
- brain module, strategy, and provider-state policy;
- runtime limits and history-window defaults;
- tool policy and selected toolsets;
- memory-space policy toggles for Crew memory;
- background review settings;
- MCP binding defaults and current binding records;
- channel/default wake policy and current binding records;
- active session ids and derived runtime graph references;
- mutable GUI-owned settings;
- import/export metadata and source asset refs.

The DB registry should be Rust-owned or Rust-validated. TypeScript and Rusty
View can orchestrate UI/admin workflows over official APIs, but they should not
silently mutate runtime graph files as the primary source of truth long term.

### File-Backed Assets

Files remain the right home for:

- `soul.md`;
- long-form static persona/instruction text;
- optional static `memory.md` prompt notes;
- starter profile templates;
- checked-in examples;
- skills and skill packages;
- human-authored docs bundled with a profile;
- exported profile bundles.

File assets are visible and portable. They are not a good primary store for
mutable runtime plumbing such as channel bindings, MCP bindings, active session
ids, or GUI-edited provider choices.

### Derived Runtime Records

Brains, sessions, scheduled jobs, channel bindings, and MCP bindings should be
derived/applied from the registry through the Rust-owned config validation
boundary. They should not remain hand-authored duplicate truth once the DB
registry is active.

## Ownership Boundary

Rust should own or canonically validate:

- registry schema and migrations;
- profile id/lifecycle/status rules;
- runtime graph expansion from registry state;
- collision checks across profile/session/agent/binding ids;
- session default/resource limit validation;
- binding identity/reference consistency;
- version/revision checks for profile edits;
- decommission/archive state transitions;
- import/export validation and registry snapshots;
- diagnostics for active registry vs file asset drift.

TypeScript should own:

- reading/writing profile asset files;
- Markdown prompt rendering;
- skill file discovery/loading;
- provider client details and brain module composition;
- admin/Rusty View API composition;
- presenting import/export plans;
- compatibility loading for old file-backed profiles during migration.

## Profile Asset Bundle

A profile asset bundle is a directory with predictable files:

```text
profile.yaml          # template/static defaults, not active runtime truth
soul.md               # long-form persona/instruction text
memory.md             # optional static prompt notes, not dense runtime memory
skills/               # optional profile-local skills
README.md             # optional human notes
```

The DB registry may reference an asset bundle by path or content fingerprint.
When a GUI edits prompt text, the API should make clear whether the change is:

- updating a DB-managed prompt override;
- updating the referenced file asset;
- creating a new exported asset snapshot.

V1 should prefer preserving file assets and storing only references plus
runtime state in DB. If later UI editing needs DB prompt overrides, add them as
explicit override fields with export support.

## Import Model

Import converts file-backed profiles into registry records.

Import should:

1. load existing `profile.json` or `profile.yaml` plus prompt assets;
2. validate runtime-affecting fields through Rust config validation;
3. create or update a profile registry record;
4. create derived runtime graph records or an apply plan;
5. preserve asset references and content fingerprints;
6. report unsupported/ambiguous fields rather than silently dropping them.

Import is not the same as activating. A profile bundle can be imported as a
template, then activated later with a session/agent/binding plan.

## Export Model

Export materializes registry state and assets for backup/review.

Export should produce:

- `profile.yaml` with stable registry/runtime fields;
- `soul.md` and `memory.md` from referenced assets or explicit DB overrides;
- skill refs or bundled profile-local skills;
- `registry.json` for fields that are not natural prompt/template config;
- optional `runtime-plan.json` showing derived brain/session/binding records;
- checksums/fingerprints for assets and source registry revision.

Export must be safe while the service is running. It should use service APIs and
consistent snapshots, not copy arbitrary DB files.

## Compatibility Plan

Phase 1: Compatibility projection

- Keep current file-backed loading.
- Add a registry design/API projection that can represent current profiles.
- Add diagnostics for file-backed profiles and missing derived runtime entries.

Phase 2: DB registry with file import

- Add registry tables and repository APIs.
- Import existing file profiles into registry records.
- Keep file loader as fallback/compatibility path.
- Create-profile API writes registry records first and can still export files.

Phase 3: Registry-authoritative runtime graph

- Runtime graph expansion reads registry records and produces brain/session/job
  binding apply plans.
- `service.json` stops being the primary active profile graph source.
- File edits require explicit import/reload to affect active runtime state.

Phase 4: Export and drift tooling

- Add export profile bundle API.
- Add diagnostics for asset drift, stale fingerprints, and registry/file
  mismatch.
- Rusty View can show which fields are active DB state vs file assets.

## Rusty View Workflow

Rusty View should use official APIs:

- list profiles from the registry;
- create profile with required identity plus optional template/source bundle;
- edit registry-owned fields through plan/apply endpoints;
- edit prompt assets through explicit asset APIs;
- preview runtime graph impact before applying changes;
- trigger config/runtime refresh/rebuild through existing guarded controls;
- export profile bundles for backup/review.

Profile create should require only unique human choices: profile id, optional
display name, optional template/source, and optional initial model/profile
choices. Defaults should come from Rust-owned profile registry defaults and the
service config validator.

## Relationship To Memory Spaces

Static `memory.md` remains prompt asset text. It is not `profile_dense` memory.

Runtime/profile/user memory lives in typed memory spaces:

- `profile_dense` for compact profile/user facts;
- future `session_memory`;
- future `roleplay_lore`.

The profile registry may store memory policy toggles and default prompt
injection settings, but memory records belong to memory-space repositories.

Export should be able to include memory-space data as an optional separate
section, with provenance and governance metadata. It should not mix dense
runtime memory back into `memory.md` without an explicit operator action.

## Decommission And Delete

Decommission means:

- mark registry profile inactive/decommissioned;
- archive/pause active sessions;
- remove or disable derived runtime bindings;
- preserve assets and memory by default.

Delete/destructive purge is a separate explicit operation and should require a
backup/export plan. V1 should avoid destructive profile deletion.

## First Implementation Slice

1. Add Rust profile registry record types and repository storage for active
   profile metadata, lifecycle status, source asset refs, and revision tokens.
2. Add import projection from existing file-backed `ProfileConfig` into a
   registry draft.
3. Add read/list admin API for profile registry records and source asset status.
4. Update create-profile planning so the official path can produce a registry
   create plan before writing files/service graph entries.
5. Add export plan API that materializes a profile bundle without changing
   active state.
6. Add diagnostics for file-backed fallback profiles and registry/file drift.

## Consequences

Positive:

- GUI-created profiles stop depending on fragile file/service-config
  choreography.
- Active runtime state gets revision checks and diagnostics.
- Humans keep transparent prompt assets and exportable profile bundles.
- Future multi-agent/multi-frontend profile management has one official path.

Costs:

- More migration machinery than pure files.
- Need clear UI language for DB-active state vs file assets.
- Export/import must be maintained carefully so profile data remains portable.
- Existing service config reload paths need compatibility while the registry
  takes over.

## Deferred Decisions

- Whether DB-managed prompt overrides are needed in addition to file assets.
- Exact bundle format for exported skills and memory-space data.
- Whether profile registry tables live in core persistence directly or in a
  module schema bundle.
- How to expose non-admin user-facing profile editing APIs.
- When to stop loading active profiles directly from files by default.
