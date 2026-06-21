# Den Router Metadata Diagnostics

Status: Initial implementation for task 3055

Date: 2026-06-20

Related docs:

- `den-work-ref-router-metadata-contracts`
- `multi-agent-adapter-architecture`
- `den-assignment-completion-evidence-loop-plan`

## Purpose

Operators need to inspect how a Den/channel/product reference maps to a Rusty
Crew runtime binding without giving the adapter coordination authority.

This slice adds a small adapter-owned metadata reader/store for
`DenRouterMetadataProjection` records.

## Implementation

`@rusty-crew/adapter-den` now exports:

- `DenRouterMetadataReader`
- `DenRouterMetadataStore`
- `DenRouterMetadataQuery`
- `createMemoryDenRouterMetadataStore()`

The in-memory store is instance-owned by the caller. It is not a global active
agent singleton.

Queries can scope by:

- binding id;
- adapter id;
- agent/session/profile runtime identity;
- provider/channel refs;
- binding status;
- limit.

## Safety

The store sanitizes provenance on write and again on read. Secret-like keys such
as token, secret, password, credential, prompt, raw output, and tool output are
redacted.

Records expose refs and status only. They do not expose provider tokens, raw
prompts, full provider payloads, or full tool outputs.

## Boundary

Router metadata is diagnostic/product-context projection. It does not:

- route messages directly;
- choose which brain wakes;
- claim or complete Den assignments;
- infer runtime completion from Den/channel state;
- replace Rust session/delegation/completion state.

## Proof

`npm run smoke:den-router-metadata` now verifies:

- router metadata creation;
- provenance redaction;
- scoped query by binding;
- scoped query by runtime/channel identity;
- degraded binding query;
- no singleton/global active agent behavior.
