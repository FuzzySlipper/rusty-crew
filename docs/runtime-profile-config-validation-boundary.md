# Runtime/Profile Config Validation Boundary

Status: design contract for review

Date: 2026-06-23

Related task: #3230

Related docs:

- `[doc: rusty-crew/brain-island-rust-ownership-audit-2026-06-23]`
- `[doc: rusty-crew/modular-brain-boundary-design-2026-06-23]`

## Purpose

Rusty Crew currently loads and mutates most service runtime/profile config in
TypeScript. That is reasonable for file parsing, prompt rendering, skills, and
provider adapters, but some config shapes directly determine Rust-owned runtime
state: sessions, brain registrations, scheduled jobs, channel bindings, MCP
bindings, profile defaults, and create-profile plumbing.

This design defines the smallest Rust-owned validation/control boundary for
durable runtime config shapes. The goal is not to move the profile system to
Rust. The goal is to make every frontend/admin path that creates or mutates
runtime-owned objects pass through one canonical validator/expander before the
service applies changes.

## Current Surfaces

### TypeScript-Owned Today

`service-runtime-config.ts` currently:

- parses `service.json`;
- validates top-level arrays for `brains`, `sessions`, `scheduledJobs`,
  `channelBindings`, and `mcpBindings`;
- expands profile `backgroundReview` into configured host scheduled jobs;
- expands profile `mcpConfig` into configured MCP binding records;
- applies profile session defaults to session config before bridge calls;
- registers brains, sessions, and configured jobs through the native bridge.

`profile-loading.ts` currently:

- loads JSON profiles or profile directories with `profile.yaml`, `soul.md`,
  and `memory.md`;
- validates model, brain, runtime, tool, MCP, background review, memory,
  session default, and channel default fields;
- loads skills and prompt fragments;
- builds profile context and tool selection inputs.

`service-host.ts` currently:

- implements admin create-profile plumbing;
- validates component ids with a local regex;
- writes profile files and `service.json` entries;
- checks duplicate profile/brain/session ids;
- reloads runtime config without creating missing sessions unless explicitly
  requested.

### Rust-Owned Today

Rust already validates bridge/runtime shapes after TS has constructed them:

- `SessionConfig` and session creation/ensure paths;
- `BrainImplementationRegistration`;
- scheduled job persistence;
- channel/MCP binding persistence records;
- resource limits, tool profile descriptors, and history windows as part of
  session state.

This protects the engine from bad bridge calls, but it is not a canonical
service config validator. It does not validate the full service graph before a
mutation is written, and it does not own create-profile defaults.

## Decision

Add a new Rust crate for service config validation and expansion:

```text
crates/core/core-config
```

The crate should be Rust-owned application/control-plane validation, not a
provider adapter and not prompt rendering. It should depend on `core-protocol`
for shared runtime types and, when useful, expose bridge-callable functions
through `core-bridge-node`.

### Why Not `core-protocol`

`core-protocol` should stay transport-free type vocabulary. It can hold stable
shared structs/enums once they are mature, but config validation has behavior:
defaults, duplicate checks, graph validation, and expansion. Putting behavior in
`core-protocol` would make the protocol crate do application work.

### Why Not `core-engine`

`core-engine` owns runtime composition. Config validation needs to run before a
mutation is accepted and before the engine applies it. Frontends and TS service
code should be able to call the validator without constructing or mutating an
engine.

### Why A New Crate

The validation boundary is service control-plane logic. It needs behavior and
tests, but it should remain independent of TS adapters, provider SDKs, and the
running engine. A small `core-config` crate gives Rust a clear home for
canonical config rules without bloating `core-protocol` or `core-engine`.

## Ownership Boundary

### Rust Owns

Rust should own validation and expansion for shapes that create, mutate, or
target Rust-owned runtime records:

- profile identity fields used by runtime records;
- brain registration ids and profile references;
- session ids, agent ids, profile ids, kinds, resource limits, history windows,
  and session defaults;
- scheduled job shape, schedule parseability, target references, executable
  kind, and host/session target requirements;
- channel binding identity/reference consistency;
- MCP binding identity/reference consistency;
- create-profile defaults that write runtime graph entries;
- duplicate/collision checks across the runtime graph;
- apply-plan summaries that say what would be created, updated, invalidated, or
  skipped.

### TypeScript Owns

TypeScript should keep ownership of:

- profile file discovery and compatibility loading (`profile.json`,
  `profile.yaml`, `soul.md`, `memory.md`);
- prompt/soul/memory rendering and role assembly;
- skill file loading and Markdown/frontmatter parsing;
- model provider SDK/client details;
- brain module implementation and provider-specific strategy logic;
- external adapter client details for Den, MCP transport, Telegram, browser,
  web, and memory services;
- admin panel rendering and frontend presentation.

### Shared/Generated

Stable schema shapes should move toward generated Rust/TS parity:

- Rust canonical structs/enums live in `core-config` or `core-protocol`
  depending on whether they are behavioral config or protocol vocabulary.
- TS imports generated or checked types rather than maintaining a separate
  handwritten shape long term.
- The bridge manifest/codegen path should eventually expose config validation
  functions to TS.

Until codegen owns parity, duplicate TS types may remain as compatibility
facades, but tests should compare TS-loaded output against Rust validation.

## Canonical Shapes

### Runtime Config Draft

Rust should validate a draft equivalent to:

```ts
interface RuntimeConfigDraft {
  profilesDir: string;
  skillsDir?: string;
  brains: BrainConfigDraft[];
  sessions: SessionConfigDraft[];
  scheduledJobs: ScheduledJobConfigDraft[];
  channelBindings: ChannelBindingConfigDraft[];
  mcpBindings: McpBindingConfigDraft[];
}
```

Draft means pre-expansion and pre-apply. It can contain user-authored entries
from `service.json`.

### Profile Runtime Metadata

Rust should not parse profile files directly in the first pass. TS should load
profiles and pass a reduced metadata map into Rust validation:

```ts
interface ProfileRuntimeMetadata {
  profileId: string;
  brain?: { module?: string; strategy?: string };
  runtime?: {
    defaultResourceLimits?: ResourceLimits;
    maxTurnDurationMs?: number;
    maxTokensPerTurn?: number;
  };
  sessionDefaults?: {
    ownerId?: string;
    maxHistoryMessages?: number;
    turnTimeoutMs?: number;
  };
  mcpConfig?: {
    bindingId?: string;
    endpointRef?: string;
    serverNames?: string[];
    transport?: string;
    toolProfile?: string;
  };
  backgroundReview?: {
    enabled: boolean;
    reviewType?: "memory" | "skills" | "combined";
    schedule?: string;
  };
  channelDefaults?: {
    wakePolicy?: "subscription" | "manual" | "disabled";
  };
}
```

This keeps Rust out of prompt files and provider-specific profile fields while
letting it validate runtime-affecting decisions.

### Expanded Runtime Plan

Rust should return an expanded plan rather than directly mutating files:

```ts
interface RuntimeConfigPlan {
  runtimeConfig: RuntimeConfigDraft;
  derivedBrains: BrainConfigDraft[];
  derivedSessions: SessionConfigDraft[];
  derivedScheduledJobs: ScheduledJobConfigDraft[];
  derivedMcpBindings: McpBindingConfigDraft[];
  diagnostics: RuntimeConfigDiagnostic[];
}
```

The plan allows TS to write files atomically, show previews in Rusty View/admin
UIs, and apply the result through existing bridge paths.

## Expansion Rules

### Move To Rust

These expansion rules affect Rust-owned runtime records and should become
canonical Rust logic:

- profile session defaults applied to session config;
- profile `mcpConfig` expanded to a session/profile-scoped MCP binding;
- profile `backgroundReview` expanded to a host scheduled job;
- create-profile defaults for brain entry, session entry, MCP config, local
  model fallback, and module selection fallback;
- duplicate checks across profile ids, brain implementation ids, session ids,
  agent ids, binding ids, and scheduled job ids;
- channel binding to session/profile/agent consistency checks;
- MCP binding to session/profile/agent consistency checks;
- scheduled job target validation and executable-shape gating.

### Stay In TypeScript

These expansion/rendering rules should stay TS:

- rendering system prompts, soul, memory, and instructions;
- reading `soul.md`, `memory.md`, skill Markdown, and skill frontmatter;
- building model provider clients;
- mapping profile model config into provider SDK options;
- discovering MCP server tools over the transport;
- making Den/Telegram/browser/web service calls.

## Create-Profile API Boundary

The official create-profile path should become a Rust-validated plan:

1. Frontend/TS submits the unique profile inputs: at minimum `profileId`, with
   optional display name, agent id, session id, implementation id, kind,
   model config, brain selection, and MCP tool profile.
2. TS loads current runtime config and existing profile runtime metadata.
3. Rust validates ids, detects duplicates, applies defaults, and returns a
   create-profile plan:
   - profile file seed metadata;
   - runtime `brains` entry;
   - runtime `sessions` entry;
   - optional MCP binding/profile MCP config seed;
   - diagnostics and warnings.
4. TS writes the profile file/directory and runtime config atomically.
5. TS triggers runtime reload/apply.

Rust should not write `soul.md` or `memory.md`; future frontends should edit
those through profile-file APIs. Rust should validate only the runtime-plumbing
parts that must stay consistent.

## Validation Rules

Initial high-value Rust validation should include:

- id format: profile, agent, session, brain implementation, job, binding;
- duplicate detection across configured and derived records;
- references: every brain/session/binding points to an existing profile
  metadata row;
- session kind is one of `full`, `worker`, or `delegated`;
- channel/MCP bindings resolve to exactly one configured session when they
  target a session implicitly;
- binding profile/agent/session fields agree with the target session;
- scheduled jobs have valid cron expressions and required target fields;
- only executable scheduled job shapes are applied to the engine;
- resource limits and history windows are non-negative and within practical
  caps;
- create-profile defaults do not overwrite existing files or runtime entries.

## Diagnostics

Validation should return structured diagnostics:

```ts
interface RuntimeConfigDiagnostic {
  severity: "error" | "warning" | "info";
  code: string;
  path?: string;
  message: string;
}
```

Examples:

- `duplicate_profile_id`
- `duplicate_session_id`
- `missing_profile_metadata`
- `binding_session_mismatch`
- `scheduled_job_not_executable`
- `profile_mcp_binding_derived`
- `background_review_job_derived`

Diagnostics should be useful to Rusty View and admin APIs without scraping
exception strings.

## API Shape

Initial bridge-callable functions should be pure validation/planning calls:

```ts
validateRuntimeConfigDraft(input): RuntimeConfigValidationResult
planCreateProfile(input): CreateProfilePlan
```

They should not mutate files, create sessions, register brains, or start jobs.
Effects remain in the existing TS service host until a later control-plane API
is designed.

## Phasing

### Phase 1: Rust Validator Crate

Create `core-config` with runtime config draft structs, profile runtime
metadata structs, diagnostics, and validation tests.

### Phase 2: Create-Profile Planning

Move create-profile id/default/duplicate logic into a Rust planning function and
call it from the TS admin path.

### Phase 3: Runtime Config Validation And Expansion

Have TS load profile metadata, call Rust validation/expansion, and then apply
the returned expanded config through existing registration paths.

### Phase 4: Generated/Checked TS Parity

Replace or guard handwritten TS config types with generated/checkable parity
from Rust shapes. Until then, add smoke tests that compare TS validation output
against Rust diagnostics for representative configs.

## Non-Goals

- Do not move prompt rendering, `soul.md`, `memory.md`, or skill Markdown
  loading into Rust.
- Do not move OpenAI/pi/Anthropic/den-router provider SDK details into Rust.
- Do not make Rust discover MCP tools or call Den/Telegram/browser APIs.
- Do not force Rust to parse every profile file format in the first pass.
- Do not make the validator mutate service files directly.
