# Rust Brain Crates

Future direct Rust brain modules live under this directory.

The lane is guarded by `governance/ownership.toml` and
`npm run smoke:rust-crate-boundaries`. Rust brain crates may depend on the
approved wake/protocol surfaces, currently:

- `rusty-crew-core-protocol`
- `rusty-crew-core-bridge-api`

They must not depend on Rust coordination internals such as `core-engine`,
`core-session`, `core-bus`, `core-body`, or `core-persistence`.

Provider-specific code belongs here only when it implements a brain module
behind the language-neutral wake contract. Coordination ownership stays in the
core crates.
