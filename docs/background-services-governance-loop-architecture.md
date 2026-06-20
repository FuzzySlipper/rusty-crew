# Background Services Governance Loop Architecture

Status: Architecture contract for task 2966

Date: 2026-06-20

## Scope

Rusty Crew background services are governed service loops around the Rust-owned
runtime. They are not a second coordination runtime and they must not bypass
the normal session, wake, queue, or action lifecycle.

This note covers the common architecture for:

- scheduler and cron loops;
- curator candidate/report loops;
- background memory and skill review;
- cleanup and reconciliation;
- evidence/result-reference posting;
- governance observation and audit;
- steer/follow-up decisions under frozen-snapshot policy.

## Core Rule

Background services may request work through Rust coordination, but they may
not directly drive brain sessions around Rust.

Allowed paths:

- publish internal Rust events;
- write durable scheduler/job/run state through typed Rust APIs;
- request a brain wake through Rust-owned wake scheduling;
- call typed admin/control APIs;
- emit display-only observation events.

Disallowed paths:

- directly invoking a TS brain because a cron tick fired;
- editing Den/channel state as if it were runtime truth;
- using pi-agent `steer()` or `followUp()` as a durable coordination queue;
- replaying old queued messages or expired background work;
- mutating skill/memory/profile state without preview, audit, and scoped
  authority.

## Ownership

Rust owns durable coordination state:

- scheduler definitions, ticks, claims, run ids, retry windows, and terminal
  outcomes;
- wake tickets and wake idempotency;
- runtime counters and reset/rebuild substrate;
- session/delegation cleanup and restart-safe reconciliation;
- queue retention/expiry state;
- typed audit records when an action changes runtime-owned state.

TypeScript owns host and platform logic:

- brain implementation and LLM/tool execution;
- profile/role assembly;
- Den, channel, MCP, browser, and filesystem adapter calls;
- curator candidate construction over skill/profile files;
- report formatting and operator-facing summaries;
- optional observation sinks.

Den and external services own product/platform facts:

- Den tasks, docs, messages, memories, and activity events;
- channel membership/presence/read cursors;
- external MCP server surfaces.

Rusty Crew may project these into runtime context, but external projections
remain anti-corruption layers, not authority.

## Loop Contract

Every background loop should have the same envelope:

1. Determine eligibility from durable state or bounded adapter reads.
2. Claim or skip work idempotently.
3. Produce a run id and audit-visible reason.
4. Execute a small bounded step.
5. Write a typed outcome: completed, skipped, failed, expired, cancelled, or
   blocked.
6. Emit optional `agent_activity.v1` observation with compact references.
7. On restart, resume from the durable outcome and never replay expired work.

Loops must be bounded by:

- max runtime per step;
- max candidates per pass;
- retry/backoff policy;
- TTL or deadline for stale work;
- explicit terminal states;
- safe no-op behavior when dependencies are unavailable.

## Scheduler And Cron

Scheduler/cron state should be Rust-owned. A cron tick should create or claim a
scheduled run record, then publish internal events or durable wake requests.
The tick should not invoke TS brains directly.

The first scheduler model should track:

- job id and kind;
- schedule expression or interval;
- enabled/paused state;
- scope: runtime, project, agent, session, or profile;
- lease/claim token;
- next due time;
- last run id and last outcome;
- retry/backoff data;
- bounded payload.

Host executors may live in TS when they need adapters, filesystem, Den, or LLM
logic. Even then, the executor receives a claimed run and reports an outcome;
it does not invent scheduler truth.

## Curator

Curator work is a governance/control capability, not a general tool escape
hatch.

Curator candidate discovery and report generation can be TypeScript-owned
because they inspect skill/profile files and compose operator-readable reports.
Durable curator state should still be typed and auditable:

- candidate batch id;
- source scope;
- snapshot reference;
- candidate kind;
- proposed mutation;
- preview diff or summary;
- approval state;
- applied outcome;
- rollback reference when applicable.

Mutation boundaries:

- default to preview/report;
- require explicit approval for destructive or broad changes;
- respect pinned/protected skills;
- write snapshots before mutation;
- archive rather than delete where possible;
- emit audit and observation breadcrumbs;
- expose rollback when the underlying storage supports it.

`curator_execute` should start narrow. It can request approved curator actions,
but it must not become a broad arbitrary filesystem, memory, or Den mutation
tool.

## Background Review

Background memory/skill review should begin with static and bounded checks:

- stale or oversized skills;
- invalid frontmatter;
- duplicate or near-duplicate skill names;
- dense profile memory cap pressure;
- missing context diagnostics;
- registry drift or unavailable tool surfaces.

LLM-backed review, when added, should run through normal brain/provider paths
with explicit profile/tool selection. It should produce findings and proposed
actions, not silently mutate source data.

Review findings should include:

- finding id;
- source refs;
- severity;
- proposed action;
- confidence;
- expiry or supersession state;
- linked curator candidate when applicable.

## Cleanup And Reconciliation

Cleanup loops must be conservative and restart-safe. They may archive or mark
terminal only through typed Rust operations.

Initial cleanup domains:

- orphan delegated sessions;
- expired delegated sessions and worker-run records;
- queued messages past TTL;
- stale adapter bindings/surfaces;
- leaked runtime buffers;
- old scheduler claims past deadline.

Cleanup must not:

- fabricate completion packets;
- replay pending messages;
- treat Den projection as required for coordination cleanup;
- overwrite existing terminal outcomes.

## Evidence And Result References

Evidence posting should prefer compact references over bulky payloads.

Good evidence:

- Den message id;
- document slug;
- commit hash;
- artifact path or content hash;
- runtime session id;
- completion packet sequence;
- observation event id.

Background loops should store enough evidence to audit why a decision happened
without embedding raw prompts, full tool outputs, secrets, or large files in
runtime state.

## Observation And Audit

`agent_activity.v1` observation is display-only. It can tell operators what a
background loop noticed or did, but it must not claim, retry, complete, wake, or
dedupe work.

Audit records are authoritative only for the state domain that owns them. For
example, a curator mutation audit explains a skill/profile file change; it does
not prove a runtime session completed.

Observation events should carry:

- compact summary;
- source domain;
- actor/profile/session when known;
- work refs and result refs;
- severity and visibility;
- bounded details.

## Steer And Follow-Up

ADR 0003 remains the default: frozen snapshot, next-wake deltas, body-owned TTL,
small caps, and expired-message drop behavior.

Any steer/follow-up implementation must:

- be body-owned rather than pi-agent-owned;
- use aggressive TTL;
- never replay expired messages;
- avoid mutating an in-flight provider stream;
- schedule a later Rust-owned wake when appropriate.

Porting pi-crew's direct `Agent.steer()` / `followUp()` bridge is not approved
by this note.

## Prime-Agent-First Runtime

Background services must not re-center Rusty Crew around worker-pool assignment
loops. The dominant model is still full/prime agents doing work through normal
sessions, with subagent delegation for context and token control.

Worker pools can later become a capacity and placement layer over the same
session/delegation/scheduler substrate. They should not be the substrate.

## Failure Posture

Background services fail closed:

- unavailable Den/channel/MCP services produce skipped/degraded outcomes;
- partial reports remain reports, not mutations;
- failed observation writes are diagnostic issues, not failed coordination;
- retries need bounded backoff;
- repeated failures should surface through admin diagnostics.

## Follow-On Task Mapping

This note is the common citation point for:

- scheduler persistence and cron execution;
- scheduled job host executors;
- curator state, reports, mutations, and admin routes;
- background review triggers and runner output;
- cleanup/reconciliation loops;
- evidence posting policy;
- governance observation/audit projection;
- steer/follow-up decision and any later implementation;
- background service diagnostics and e2e proofs.
