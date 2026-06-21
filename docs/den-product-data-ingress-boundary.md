# Den Product Data Ingress Boundary

Status: Initial implementation for task 3054

Date: 2026-06-20

Related docs:

- `den-assignment-completion-evidence-loop-plan`
- `den-work-ref-router-metadata-contracts`
- `multi-agent-adapter-architecture`

## Purpose

Den product records can enter Rusty Crew as bounded context and references.
They must not become an adapter-owned assignment queue.

This slice adds an observe-only Den product-data ingestion helper in
`@rusty-crew/adapter-den`.

## Implementation

`den-product-ingress.ts` exports:

- `toDenProductDataUpdate()`
- `denProductReferenceWorkRef()`
- `ingestDenProductReference()`

The ingestion helper accepts Den product refs such as tasks and assignments,
normalizes them into:

- Rust-facing `DenDataUpdate`
- Den-sourced `work_ref.v1`
- redacted provenance

Then it calls the provided `injectDenDataUpdate()` ingress.

## Denied Operations

The helper only accepts `operation: "observe"` or an omitted operation. It
denies:

- `claim`
- `complete`
- `retry`
- `expire`

Those actions are lifecycle/coordination decisions and must be routed through
typed Rust/control APIs or explicit Den-domain operations later. They are not
adapter-side background loop behavior.

## Failure Behavior

If Rust ingress is unavailable, ingestion returns `status: "degraded"` with a
safe reason code. It does not retry indefinitely, claim work, or create an
assignment loop.

## Proof

`npm run smoke:den-product-ingress` proves:

- assignment refs become `DenDataUpdate` plus `den:assignment:*` work refs;
- secret-like provenance is redacted;
- `claim` is denied and does not inject an update;
- ingress failure returns degraded instead of throwing or claiming work.
