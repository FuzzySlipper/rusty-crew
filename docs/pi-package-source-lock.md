# Retired pi Package Source Lock

Rusty Crew no longer uses the `earendil-works/pi` npm packages as runtime
dependencies. The production `pi-agent-core` module id now resolves to the Rust
pi-agent brain behind the neutral wake/stream/action/provider-state contract.

This file is retained as historical audit context for work that preceded the
Rust pi-agent cutover. Do not add these packages back to manifests to satisfy a
current feature.

## Retired Pin

- Repository: `https://github.com/earendil-works/pi`
- Commit: `6e6ce70caf3328683517b0e308fdbbc6d1c1abc9`
- `@earendil-works/pi-agent-core`: `0.79.8`
- `@earendil-works/pi-ai`: `0.79.8`

Older local checkout paths in audit docs are historical context only.
