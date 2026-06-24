# Rust Brain Crate Firewall

Status: implemented governance check for task #3295
Date: 2026-06-24

Rusty Crew supports direct Rust brain modules behind the language-neutral wake
contract. That removes the physical protection previously provided by the
TypeScript/Rust boundary, so Rust brain crates need a crate-level firewall.

## Approved Surface

Rust brain crates should live under `crates/brains/` and depend only on the
approved protocol/wake surfaces:

- `rusty-crew-core-protocol`
- `rusty-crew-core-bridge-api`

These surfaces expose wake inputs, stream items, brain events, action batches,
and bridge-facing helper types. They do not expose coordination ownership.

## Forbidden Coordination Dependencies

Rust brain crates must not depend on:

- `rusty-crew-core-engine`
- `rusty-crew-core-session`
- `rusty-crew-core-bus`
- `rusty-crew-core-body`
- `rusty-crew-core-persistence`
- `rusty-crew-core-config`
- `rusty-crew-core-tool-registry`
- native/mock/codegen bridge implementation crates

Provider modules own provider API calls, request construction, response parsing,
and provider wire-state payload semantics. They do not own session lifecycle,
wake scheduling, queues, bus routing, action validation, delegation lifecycle,
or coordination persistence.

## Machine Check

Run:

```bash
npm run smoke:rust-crate-boundaries
```

The check reads `governance/ownership.toml`, scans Cargo manifests under
`crates/`, and fails if an exact crate rule or lane rule is violated.

The `rust-brain-module` lane currently matches `crates/brains/`. A future
Responses crate under that path will fail the check immediately if it reaches
for coordination internals.
