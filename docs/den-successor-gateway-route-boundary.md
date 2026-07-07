# Den Successor Gateway Route Boundary

Status: Implemented for task 4644

Date: 2026-07-07

## Purpose

Rusty Crew talks to Den successor services through the `adapter-den`
Successor Gateway client. Den Gateway route/version churn belongs at that
adapter boundary and must not leak into Rust coordination crates, brain
runtime logic, or profile configuration.

## Prefix Ownership

The adapter owns the successor Gateway API prefix:

- default prefix: `/v1`;
- environment overrides:
  - `DEN_SUCCESSOR_GATEWAY_API_PREFIX`;
  - `DEN_GATEWAY_API_PREFIX`.

The service-facing config mirrors the same field as
`denSuccessorGateway.apiPrefix`. A profile should not specify successor Gateway
paths directly.

`GET /health` remains unversioned. Runtime, observation, delivery, and
conversation routes are composed from the configured prefix inside
`ts/packages/adapter-den/src/successor-gateway.ts`.

## Update Procedure

When Den successor Gateway changes its API route prefix:

1. Prefer changing deployment config with `DEN_GATEWAY_API_PREFIX`.
2. If individual route names change, update the route table/path composition in
   `adapter-den`; do not patch call sites outside the adapter.
3. Update the Den successor Gateway smoke so it still proves a non-default
   prefix.
4. Keep external cassettes on the captured live route paths unless the captured
   Den service has actually changed.

## Validation

Run:

```sh
npm run smoke:successor-gateway -w @rusty-crew/adapter-den
npm run smoke:successor-gateway-cassettes -w @rusty-crew/adapter-den
```

The first smoke uses a non-default prefix (`/edge/v1`) to prove route
composition is centralized. The cassette smoke validates the current captured
Den `/v1` envelopes after redaction.
