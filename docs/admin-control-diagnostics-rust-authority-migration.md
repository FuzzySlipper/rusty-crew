# Admin Control, Diagnostics, And Command Authority Migration

Status: implementation plan for Den task 4586
Date: 2026-07-07

## Purpose

Rusty Crew already has useful operator surfaces: guarded admin control routes,
read-only diagnostics routes, slash commands, API capability catalogs, and
storage query catalogs. The current TypeScript boundary is intentionally
framework-neutral, but it has grown large enough that route glue, presentation,
catalog metadata, and lifecycle authority are too easy to blur.

This plan continues the TS authority reduction work. Rust should own durable
runtime/control decisions and generated contract truth. TypeScript should keep
HTTP envelopes, browser/channel presentation, autocomplete, adapter projection,
and thin dispatch glue.

## Current Surfaces

The main TypeScript surfaces are:

- `admin-control-api.ts`: guarded admin control route parser, audit envelope,
  and executor dispatch.
- `admin-diagnostics-api.ts`: read-only diagnostics route envelope and filters.
- `api-command-registry.ts`: API capability metadata plus chat slash command
  descriptors.
- `slash-command-router.ts` and `slash-command-responses.ts`: chat command
  interception and readable command output.
- `runtime-diagnostics.ts`: aggregate health/readiness projection over runtime,
  queue, adapter, tool, persistence, and observation inputs.
- `storage-query-catalog.ts`: admin-facing storage query catalog and guard
  metadata.

Rust already owns several pieces of durable truth:

- `core-config` validates runtime config and create-profile plans.
- `core-persistence` owns storage diagnostics, module schema diagnostics, query
  catalog entries, provider-state diagnostics, and backend capability snapshots.
- `core-tool-registry` validates tool metadata.
- `core-engine` and the bridge manifest expose sessions, scheduler jobs,
  provider-state diagnostics, storage diagnostics, and other runtime readbacks.

There is not currently a separate `service-host` crate. Host authority is split
among core crates, bridge operations, and TypeScript service composition, so
these tasks should move one command/readback family at a time instead of waiting
for a large host-crate rewrite.

## Target Boundary

### Rust Owns

- Valid control-plane command kinds, target identity rules, idempotency keys,
  lifecycle preconditions, and durable mutation plans.
- Durable health/readiness facts that come from sessions, queues, persistence,
  scheduler state, wake state, provider state, and config validation.
- Capability and query catalog contract truth, either directly in Rust or
  through generated artifacts from Rust-owned manifests.
- Stable reason codes for unsupported, denied, degraded, and unavailable
  control/diagnostics states.

### TypeScript Owns

- HTTP route matching and response envelopes.
- Bearer-token checks at the service edge.
- Operator-friendly redaction and pagination for display payloads.
- Slash command parsing, autocomplete, and read-only text rendering.
- Adapter diagnostics projection for external dependencies.
- Dispatch glue that calls Rust/bridge operations or reports unsupported
  controls when the Rust operation does not exist.

## Migration Slices

1. Move admin control command planning to Rust for one small command family,
   then expand family-by-family. `/new`, MCP reload, scheduler controls, and
   shutdown should become typed Rust control plans rather than TS-only command
   objects.
2. Split diagnostics into durable Rust-owned read models and TS adapter
   presentation. Runtime/session/queue/storage/provider/config truth should be
   projected from Rust or Rust-validated inputs; adapter health can stay as TS
   projection but must remain labeled as external.
3. Generate or Rust-validate API capability and slash command catalogs. TS may
   present the catalog, but new routes/commands should fail a ratchet if they
   are not represented in the authoritative manifest/fixture.
4. Move storage query catalog truth to the Rust module schema/query catalog
   registry. TS can keep UI descriptions and request envelopes, but not decide
   which logical stores or backend requirements exist.
5. Keep slash commands as UI/channel control intents. Mutating slash commands
   should resolve to Rust-owned control plans; read-only slash responses should
   consume diagnostics read models instead of ad hoc TS runtime inspection.
6. Add deterministic and live certification that admin controls, diagnostics,
   slash commands, and capability catalogs agree across the API, Rusty View, and
   debug service.

## Non-Goals

- Do not make slash commands model-callable tools.
- Do not let diagnostics mutate or repair runtime state.
- Do not make Den observation or Den product data authority for Crew control
  decisions.
- Do not preserve TS-only lifecycle fallbacks once a Rust control plan exists.
- Do not hide missing Rust operations behind permissive no-op executors.

## Acceptance For The Series

- A mutating admin or slash command cannot perform lifecycle work solely through
  TypeScript validation and private executor code.
- Capability, command, and storage-query catalogs have a Rust/generated
  validation path.
- Diagnostics clearly label durable Rust-owned state versus TS adapter/external
  projections.
- Rusty View and agent-facing command surfaces can query current commands and
  capabilities without relying on stale prose docs.
