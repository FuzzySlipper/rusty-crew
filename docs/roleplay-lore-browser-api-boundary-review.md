# Roleplay Lore Browser API Boundary Review

Date: 2026-07-06

Task: `#4336`

## Current Slice

The roleplay/lore browser route handling lives in
`ts/packages/brain-island/src/service-roleplay-routes.ts`. `service-app.ts`
keeps only request dispatch and adapts the full service state into a narrow
`RoleplayRouteContext`.

That context intentionally exposes only:

- the native bridge, which is the storage-backed authority surface;
- the profile config directory, for the current narrator config file write;
- the service clock;
- runtime config reload after narrator config writes;
- session lookup and chat event preview helpers.

## TS Glue

These pieces are acceptable TypeScript glue for now:

- browser route matching and method dispatch;
- JSON body parsing and browser-safe response projection;
- query parameter parsing for browser requests;
- narrator config request shaping;
- character/session browser metadata stored through simple-kv;
- prompt-context assembly from stored roleplay session metadata and selected
  character fields.

These are presentation and adapter concerns. They should stay small and should
not grow new storage policy.

## Rust And Storage Authority

The following semantics remain behind the Rust/native bridge and must not be
reimplemented as TypeScript policy:

- lore entry persistence and replacement revision checks;
- lore layer persistence and archive/write-policy enforcement;
- layer-entry links, promotion, provenance, and supersession relationships;
- chat-layer ordering and enabled state;
- lore query paging over the storage backend;
- simple-kv persistence for roleplay profile/session metadata.

The route module may compose bridge calls for browser workflows, but durable
integrity decisions belong in storage/domain code.

## Generated Contract Candidates

The extracted module is a good candidate for a generated API contract pass once
the route shape settles. Highest-value schemas:

- roleplay lore entry search request and response;
- lore entry create/patch/promote envelopes;
- lore layer create/update/list envelopes;
- chat-layer set/toggle/reorder envelopes;
- character/session/narrator browser records.

Generated contracts should become the shared Rust/View/TS source for these
browser API shapes rather than letting `service-roleplay-routes.ts` remain the
only executable specification.

## Further Rust Candidates

The next useful Rust/domain moves are:

- move layer-scoped search planning and paging metadata into a storage/domain
  API that can return exact and inexact totals consistently;
- move promotion preflight into one bridge operation so source-layer ambiguity,
  target policy, and record creation are atomic from the route's point of view;
- give roleplay character/session metadata a first-class storage module if
  roleplay grows beyond browser metadata.

These are not required for the current extraction, but they are the pressure
points most likely to drift if more roleplay features land only in TypeScript.
