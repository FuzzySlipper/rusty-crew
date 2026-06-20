# Evidence Result Reference Policy

Status: Design contract for task 2978

Date: 2026-06-20

## Scope

Background and governance loops need evidence, but Rusty Crew should not revive
old Den breadcrumb coupling or embed bulky artifacts in observation events.

This policy defines what counts as evidence, how evidence maps to
`agent_activity.v1` `result_ref`, and how channel/evidence posting should flow
through adapter projections.

## Evidence Principles

Evidence is a durable or inspectable reference that explains why a loop made a
decision or what output it produced.

Evidence should be:

- reference-first, not blob-first;
- stable enough for later audit;
- safe for diagnostics and observation;
- bounded in size;
- redacted before publication;
- scoped to the owning domain.

Evidence is not runtime authority by itself. The owning state domain remains
authoritative: Rust runtime records, Den product data, Git commits, artifacts,
or adapter provider records.

## Evidence Types

Accepted evidence refs:

- runtime session id;
- runtime event sequence;
- completion packet sequence or session id;
- scheduler job id and run id;
- queued message id;
- worker/delegation run id;
- Den message id;
- Den document slug;
- Den task/comment id;
- Den memory id/slug;
- observation event id;
- diagnostics bundle id;
- review finding id;
- curator candidate batch id;
- commit hash;
- artifact path plus content hash;
- channel binding id and external message id.

Unsafe evidence payloads:

- raw prompts;
- full tool output;
- secrets or environment dumps;
- large file contents;
- raw SQL;
- executable command strings;
- provider tokens or transport cursors beyond safe opaque ids.

## Result Reference Shape

The current `agent_activity.v1` producer supports a compact result ref:

- `document_slug`
- `message_id`
- `commit`
- `artifact_path`

Background loops may also need richer internal refs. Use an internal
`EvidenceRef` shape before projecting to observation:

```ts
type EvidenceRef = {
  kind:
    | "runtime_session"
    | "runtime_event"
    | "completion_packet"
    | "scheduler_run"
    | "queued_message"
    | "delegation_run"
    | "den_message"
    | "den_document"
    | "den_task"
    | "den_memory"
    | "observation_event"
    | "diagnostics_bundle"
    | "review_finding"
    | "curator_candidate_batch"
    | "commit"
    | "artifact"
    | "channel_message";
  id: string;
  label?: string;
  content_hash?: string;
  source_domain: "runtime" | "den" | "git" | "artifact" | "channel";
};
```

Projection to `agent_activity.v1.result_ref` should use the best supported
field and keep any richer refs in the loop's own run/report record.

## Work Reference Shape

Use `work_ref` for what work the evidence belongs to:

- project id;
- task id;
- assignment id;
- run id;
- review round id;
- channel id/message id;
- runtime session id.

Do not overload `result_ref` to mean both work identity and output evidence.

## Channel And Den Posting

Channel evidence posting must go through normalized channel outbound projection:

1. Background loop completes with evidence refs.
2. Optional observation event is emitted with compact `work_ref` and
   `result_ref`.
3. Channel adapter decides whether a display projection should be sent for the
   binding/surface.
4. Adapter sends provider-specific message using normalized outbound message
   shape.
5. Projection failure degrades adapter diagnostics; it does not roll back the
   runtime job/run outcome.

No background loop should call Den Channels-specific APIs as its core evidence
mechanism. Den Channels is one adapter provider.

## Size And Redaction

Evidence summaries:

- max 240 chars in observation summary;
- max 2,000 chars in scheduler/curator report excerpts unless a task-specific
  doc says otherwise;
- large evidence should become an artifact with content hash;
- secret-like values should be redacted before storage and projection.

Observation payloads should avoid:

- raw model prompts;
- full command lines;
- full environment variables;
- full file diffs;
- full HTTP payloads;
- raw provider response bodies.

## Retention

Retention follows the owning domain:

- Rust runtime facts follow runtime retention/maintenance policy.
- Den docs/messages/tasks follow Den retention.
- Git commits follow repository retention.
- Artifacts follow artifact store retention.
- Observation events are display projections and may have shorter retention.

If an observation event expires, the scheduler or curator run record should
still retain enough compact refs to explain the action.

## Background Loop Requirements

Every background/governance loop should return:

- short summary;
- one or more evidence refs where possible;
- changed count;
- skipped count;
- safe error/ref on failure;
- observation publication status when attempted.

Loops that mutate state must include before/after refs or a snapshot/rollback
ref where possible.

## Current Extension Point

Before adding richer fields to `AgentActivityResultRef`, prefer:

- storing rich refs in the scheduler/curator/review run output;
- projecting the most operator-useful compact field into `result_ref`;
- adding a bounded `artifact_path` when the evidence is too large.

If repeated projections lose important information, extend
`AgentActivityResultRef` deliberately with a smoke covering redaction and
bounded size.
