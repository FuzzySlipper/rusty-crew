# ADR 0001: Current pi Package Source

## Status

Accepted.

## Context

Several planning docs mention historical local checkouts such as
`/home/research/pi-fleet/pi` or version-skewed Rust ports. Those references
were useful for audits, but they should not steer new implementation work.

## Decision

The TypeScript brain island uses the current `earendil-works/pi` source at
`https://github.com/earendil-works/pi` and the `@earendil-works/pi-*` package
names. Older local paths are historical evidence only.

The Rust coordination core still does not call LLM providers directly. It
invokes the TS brain island through the bridge boundary.

## Consequences

- Scaffold package metadata points brain-island dependency resolution at the
  current source assumption instead of an old local checkout.
- Audit docs remain useful for dependency-shape history, but unified
  architecture plus this ADR win when source-location guidance conflicts.
- Exact package-manager syntax for consuming monorepo packages can be decided
  when install tooling is chosen.
