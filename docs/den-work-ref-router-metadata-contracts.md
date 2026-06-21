# Den Work Ref And Router Metadata Contracts

Status: Initial contract slice for task 3052

Date: 2026-06-20

Related docs:

- `den-assignment-completion-evidence-loop-plan`
- `multi-agent-adapter-architecture`
- `evidence-result-reference-policy`

## Purpose

Den tasks, assignments, messages, and documents are useful work context, but
they are not Rusty Crew coordination authority. The first contract slice adds
structured references so adapter projections can point at Den product data
without turning it into a runtime queue.

## Shared Contracts

`@rusty-crew/contracts` now defines:

- `WorkReference`: compact refs for the work a projection belongs to.
- `ResultReference`: compact refs for evidence or output produced by a loop.
- `DenRouterMetadataProjection`: a display/diagnostic projection connecting a
  binding and runtime identity to Den work refs, result refs, provider refs,
  tool profile, MCP surface refs, status, and provenance.

These are deliberately reference-first. Large payloads, secrets, raw prompts,
and raw tool outputs do not belong in the records.

## Adapter-Den Helpers

`@rusty-crew/adapter-den` now exports:

- `denProductWorkRef()`
- `createDenRouterMetadataProjection()`
- `sanitizeRouterMetadataProvenance()`

The helper creates Den-sourced `work_ref.v1` records and redacts sensitive
provenance keys such as tokens, secrets, prompts, credentials, and raw tool
outputs.

## Boundary

This slice does not implement assignment polling, claiming, completion, or
worker-pool cleanup. Those remain later work. The contract only gives future
adapter projections a safe language for saying "this runtime/session/channel
activity is related to this Den product ref."

## Proof

`npm run smoke:den-router-metadata` verifies:

- Den assignment/task refs become `work_ref.v1` values;
- completion packet refs can be carried as `result_ref.v1`;
- runtime binding identity and provider refs are preserved;
- secret-like provenance keys are redacted.
