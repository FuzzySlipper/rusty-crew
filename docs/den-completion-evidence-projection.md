# Den Completion And Evidence Projection

Status: Initial implementation for task 3053

Date: 2026-06-20

Related docs:

- `den-assignment-completion-evidence-loop-plan`
- `den-work-ref-router-metadata-contracts`
- `evidence-result-reference-policy`
- `outbound-channel-projections`

## Purpose

Runtime completion and delegation lifecycle evidence should be visible in Den
surfaces without making Den the completion authority.

This slice adds a small Den adapter projection path that turns Rust runtime
events plus structured work/result refs into bounded channel activity
projections.

## Implementation

`@rusty-crew/contracts` extends `NormalizedChannelActivityProjection` with:

- `workRefs?: WorkReference[]`
- `resultRefs?: ResultReference[]`

The existing singular string fields stay in place:

- `workRef`
- `resultRef`

Those singular fields are populated from the first structured ref so existing
Den Channels provider metadata stays compact and backwards-compatible.

`@rusty-crew/adapter-den` exports:

- `projectCompletionEvidenceToChannelActivity()`
- `dispatchCompletionEvidenceProjection()`
- `completionPacketResultRef()`
- `runtimeSessionWorkRef()`

## Boundary

This projection path:

- consumes runtime events and explicit refs;
- emits bounded activity projection records;
- carries Den task/assignment refs as `work_ref`;
- carries completion packet/runtime-event refs as `result_ref`;
- catches sink failures and reports degraded dispatch.

It does not:

- claim or complete Den assignments;
- infer completion from channel prose;
- roll back or block Rust completion when Den projection fails;
- create worker-pool source or cleanup behavior.

## Proof

`npm run smoke:den-completion-evidence` proves:

- completion packet evidence produces structured and compact result refs;
- Den task/assignment refs are preserved as work refs;
- summary text is bounded before provider projection;
- failed evidence projection returns degraded dispatch while runtime completion
  remains accepted;
- later successful projection can still publish delegation lifecycle evidence.
