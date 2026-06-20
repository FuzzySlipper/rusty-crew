# Background Memory Skill Review Triggers

Status: Design contract for task 2974

Date: 2026-06-20

## Scope

Background memory/skill review is a governed scheduler job, not a free-floating
worker. It reviews dense profile memory, skill files, and related diagnostics
for drift, risk, or cleanup opportunities.

The first implementation should be static and bounded. LLM-backed review may be
added later only through normal brain/provider paths with explicit profile and
tool selection.

## Review Types

### `memory`

Reviews dense profile memory and Den memory diagnostics.

Static checks:

- dense profile memory record cap pressure;
- oversized record content;
- duplicate or near-duplicate memory keys;
- stale records by updated timestamp;
- invalid metadata shape;
- Den memory client unavailable while policy expects it.

### `skills`

Reviews profile skill roots and selected skill metadata.

Static checks:

- invalid frontmatter;
- duplicate or unsafe slugs;
- oversized skill bodies;
- missing profile-selected skills;
- pinned/protected skill counts;
- skills without summaries;
- potentially duplicated titles/summaries;
- skill root unavailable or not readable.

### `combined`

Reviews interaction between memory, skills, and role assembly.

Static checks:

- missing memory/skills diagnostics;
- selected memory/skill tools unavailable;
- dense profile memory content that appears to contain task progress/todos;
- skill text that appears to encode temporary project facts better suited for
  Den docs/tasks;
- role assembly section bloat from too many selected skills or memory records.

## Trigger Sources

Supported trigger sources:

- cron schedule: periodic bounded scan;
- explicit admin request: run now/dry run with reason;
- profile policy: review enabled for selected profile/scope;
- runtime counters: threshold crossing, such as many tool errors or memory
  writes;
- curator follow-up: rescan after approved curation;
- diagnostics degradation: missing skill root, unavailable memory client, cap
  pressure.

All triggers flow through scheduler run records. They should not directly call
review code without a claim/run id.

## Debounce And Anti-Spam

Review jobs must avoid noisy repeated findings.

Rules:

- minimum interval per scope/profile/review type;
- suppress duplicate finding fingerprints until state changes;
- cap findings per run;
- cap observation events per run;
- prefer one summary observation with result refs over per-finding channel
  chatter;
- do not post channel evidence by default; let adapter projections decide.

Suggested defaults:

- cron interval: daily for broad scan, hourly for diagnostics degradation;
- max candidates per run: 100;
- max findings per run: 25;
- observation summary only unless severity is warning/error;
- duplicate suppression window: 7 days or until source content hash changes.

## Review Run State

A review run should record:

- run id;
- review type;
- trigger source;
- profile/scope;
- candidate count;
- finding count;
- skipped count;
- finding fingerprints;
- result refs;
- safe error;
- started/finished timestamps.

Run state belongs to scheduler/governance persistence. It should not store raw
prompts, full skill bodies, or full memory content.

## Finding Shape

A finding should include:

- finding id;
- fingerprint;
- review type;
- source refs;
- severity: info, warning, error;
- confidence;
- summary;
- proposed action;
- candidate kind;
- expiry/supersession state;
- optional curator candidate ref.

Source refs should follow the evidence/result-reference policy:

- skill slug/source path hash;
- profile id;
- dense profile memory key/revision;
- diagnostics bundle id;
- Den memory id/slug;
- runtime counter summary ref.

## LLM Review Rules

LLM review is off by default.

When enabled later:

- run through normal Rusty Crew brain/provider path;
- use a bounded prompt assembled from summaries and refs;
- avoid raw full prompt dumps and large skill bodies;
- emit findings/proposals, not direct mutations;
- use a review profile with read-only tools by default;
- record model/provider and prompt artifact hash, not full prompt text.

## Mutations

Background review does not mutate memory or skills directly.

Allowed outputs:

- finding;
- curator candidate;
- diagnostics report;
- observation event;
- admin recommendation.

Actual mutations must go through curator governance or explicit skill/memory
tools with approval and audit.

## First Scheduled Job

The first concrete job should be:

`runtime.review.memory_skills`

Payload:

- `schema_version`
- `review_type`: memory, skills, or combined
- `profile_id`
- `skills_root`
- `include_dense_profile_memory`
- `include_den_memory_diagnostics`
- `max_candidates`
- `max_findings`
- `llm_review_enabled`
- `dry_run`
- `reason`

Executor: TypeScript host, because it reads skill files and may call Den/memory
diagnostic surfaces. The scheduler run and eventual findings state remain
governed background-service state.

## Implementation Order

1. Implement static skill diagnostics candidates.
2. Add dense profile memory cap/size/staleness candidates.
3. Add finding fingerprinting and duplicate suppression.
4. Add report/result refs and observation summary.
5. Create curator candidates for safe proposed actions.
6. Add optional LLM review only after the static runner is stable.
