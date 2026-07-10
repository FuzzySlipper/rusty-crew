# ADR 0024: Generated Protocol And Capability Contract Sources

Status: Accepted for task 5393

Date: 2026-07-10

## Context

Rusty Crew has good contract checks but still maintains several facts more than
once:

- Rust protocol structs and enums are mirrored by authored TypeScript types in
  `ts/packages/contracts/src/index.ts`;
- `bridge-manifest.toml`, Rust `OPERATION_NAMES`, and TypeScript
  `manifestOperationNames` repeat the bridge operation inventory;
- the generated wire fingerprint is copied into a TypeScript string export;
- `api-command-registry.ts`, slash-command routing, route-table coverage, and
  OpenAPI-adjacent documents repeat public capability metadata.

Parity checks reduce the chance of unnoticed drift, but they do not make the
ownership direction clear. Adding another general-purpose binding framework
would also be the wrong default. Rusty Crew needs reviewable generated data and
types, not an opaque generated client or a runtime framework that owns the
service boundary.

Different artifact families have different natural sources. This ADR assigns a
canonical source and generation direction to each family.

## Decision

Rusty Crew will use concern-specific canonical sources and deterministic,
committed generated artifacts.

No generated artifact is edited by hand. No legacy mirror remains after its
consumer has moved to the generated greenpath.

### Canonical Source Matrix

| Concern | Canonical source | Generated or checked output | Authored code that remains |
| --- | --- | --- | --- |
| Neutral runtime DTO fields, enum variants, and serde tags | Rust types in `crates/core/core-protocol` and approved neutral brain/runtime crates | `ts/packages/contracts/src/generated/core-protocol.ts` plus a reviewable schema inventory | branded aliases, ergonomic helpers, and TS-only adapter/UI types that do not repeat wire fields |
| Bridge operation names, ordering, direction, and surface metadata | `crates/bridge/core-bridge-api/bridge-manifest.toml` | `ts/packages/contracts/src/generated/bridge-manifest.ts`; Rust `OPERATION_NAMES` is checked/generated from the same manifest | bridge wrapper implementations and explicit ergonomic method names |
| Bridge wire fingerprint | ordered manifest plus Rust-authored wire fixtures, calculated by `core-bridge-codegen` | `bridge-wire-shape-fingerprint.txt` and the fingerprint export in generated bridge metadata | stale-binary error presentation |
| Native JSON binding surface | Rust napi methods plus the bridge manifest | napi `native/index.d.ts`, native mapping inventory, and exact manifest/native checks | Node addon loading, JSON invocation, explicit conversions, and validation calls |
| Public HTTP and slash-command metadata | one declarative TypeScript capability source under `brain-island` | browser readbacks, route/command coverage inventories, and OpenAPI documents | explicit route handlers, command execution, auth checks, and service composition |
| Runtime wire validation | Rust JSON Schemas for generated neutral DTO families, with explicit TS validation adapters at bridge boundaries | committed schema inventory and generated TS validator metadata | validation invocation, ergonomic error messages, and boundary-specific refinements |
| OpenAPI | declarative public capability source plus explicitly referenced request/response schemas | committed OpenAPI 3.1 artifacts | explicit `fetch`/SSE clients and server handlers; no generated transport client is required |

## Neutral Protocol Generation

Neutral protocol generation will use targeted `schemars` derives on the Rust
types selected for public wire generation. `schemars` is chosen because Rusty
Crew needs JSON-schema evidence for serde field names, enum values, tagged enum
shape, required/optional fields, and later runtime/OpenAPI consumers. Direct
TypeScript-only generators such as `ts-rs` or `specta` are not adopted as a
blanket framework: they would solve the first TS declaration output but would
not provide the shared schema artifact needed by bridge validation and OpenAPI.

`core-bridge-codegen` owns a small deterministic schema-to-TypeScript emitter
for the supported neutral subset. Unsupported schema constructs fail generation
with a type and path instead of degrading to `any` or an untyped object.

Initial generated families are:

- brain wake, event, action, stream, and provider-state records;
- sessions, resource limits, delegation, and completion records;
- channel and MCP binding records;
- scheduler jobs, runs, and status records;
- memory-space, proposal, activity, and compaction records.

Generation is family-based. A family moves only when its consumers import the
generated definitions and the old authored wire declarations are deleted. A
temporary duplicate is acceptable only inside one implementation commit; it is
not a supported compatibility path.

## Bridge Metadata Generation

`bridge-manifest.toml` remains the readable bridge inventory. The generated
TypeScript bridge module exports:

- ordered operation names;
- the operation-name union type;
- manifest version;
- the calculated wire-shape fingerprint;
- reviewable operation metadata needed by checks.

The TypeScript contracts package re-exports this generated module. It no longer
contains an authored operation array or fingerprint literal. Native surface and
fixture checks consume the generated module, so operation add/remove/rename and
fingerprint changes cannot be repaired by updating an unrelated hand-written
list.

The checked-in fingerprint text file remains generated evidence used by Rust
and stale native-addon checks. It is not an independently authored source.

## API And Slash-Command Declarations

Public API and command declarations remain TypeScript code-as-config because
the service HTTP boundary, explicit route handlers, browser projection, and
slash-command execution are TypeScript-owned. The declaration source contains
metadata, not execution callbacks or hidden dispatch.

Each slash command declaration includes:

- canonical name and aliases;
- description and stability;
- positional/named argument schema;
- autocomplete source metadata;
- mutation and auth classification;
- optional linked API capability and Rust planner operation.

Each API capability declaration includes:

- stable capability id;
- method and path template;
- description, tags, stability, auth, and mutation classification;
- request/response schema references;
- optional linked command and Rust planner operation;
- OpenAPI inclusion policy.

Route and command execution remain explicit in their handlers. CI checks both
directions:

- every public descriptor resolves to a real route or command handler;
- every non-exempt public route and slash-command handler has one descriptor.

Browser readbacks, autocomplete metadata, route coverage, and OpenAPI are
derived from this declaration source. Existing response envelopes stay stable
unless a task explicitly makes a breaking API change.

## Generated Artifact Rules

Generated artifacts must be:

- deterministic across clean checkouts;
- committed and readable in code review;
- formatted by the repository toolchain;
- headed with their source and regeneration command where the format allows;
- checked by an exact regenerate-and-compare command in `verify:offline`;
- free of timestamps, absolute paths, local environment values, and secrets.

Generation must be atomic from the caller's perspective. A failed generator
must not leave a partially updated artifact set.

Generated TypeScript may contain interfaces, unions, readonly inventories, and
schema metadata. It must not contain an HTTP client, hidden service locator,
ambient registry, provider SDK wrapper, or route execution logic.

## Clean-Break Deletion Policy

When a generated family lands:

1. update all production consumers to the generated import;
2. delete the authored mirror in the same task;
3. delete mirror-specific parsing/check code that no longer has an owner;
4. retain only ergonomic aliases that are defined in terms of generated types;
5. reject stale imports in CI where a narrow source check is useful.

Do not keep aliases, fallback reads, compatibility exports, or old generated
locations merely to ease current test-data migration. Current service data is
disposable. Future public compatibility requires an explicit versioned contract
and migration task, not an accidental second source.

## Reviewability And Ownership

Rust protocol changes are reviewed first as Rust semantics, then as generated
schema/TypeScript diffs. A generated diff that changes unexpectedly is a signal
to inspect serde/schema behavior, not something to normalize away.

The contract steward lane owns generation and cross-language drift gates:

- Rust protocol owners change canonical DTOs;
- bridge owners change the manifest and fixture families;
- TypeScript service owners change public capability declarations and explicit
  handlers;
- frontend consumers use generated types and queryable readbacks through
  explicit transports.

## CI And Regeneration Commands

The implementation tasks establish these greenpaths:

```bash
# Neutral Rust protocol schemas and TypeScript declarations
npm run codegen:protocol-contracts
npm run check:protocol-contracts

# Bridge manifest metadata and fingerprint export
npm run codegen:bridge-contracts
npm run check:bridge-contracts

# Public API, command, capability, and OpenAPI artifacts
npm run codegen:api-capabilities
npm run check:api-capabilities

# Existing broad gates remain authoritative
npm run verify:offline
npm run test:postgres-backend
```

The check commands generate into memory or a temporary location and compare
exactly with committed output. They do not rewrite the worktree.

## Consequences

- Rust field, enum, and serde-tag changes in selected neutral families produce
  a generated diff or fail CI.
- Bridge operation and fingerprint mirrors disappear from authored TypeScript.
- API and slash-command discovery share one declaration source while execution
  stays explicit.
- Rusty View can consume generated types and browser-readable capability data
  without adopting a generated client.
- Adding a new generated family requires deliberate schema support, but this
  cost is paid once instead of through permanent cross-language mirrors.

## Non-Goals

- Do not generate route handlers, command execution, fetch clients, SSE state
  machines, provider clients, or tool implementations.
- Do not expose all internal Rust types as public contracts.
- Do not require one generator or schema library for every artifact family.
- Do not emit `any` as an escape hatch for unsupported Rust schema shapes.
- Do not preserve authored contract mirrors as legacy fallbacks.
