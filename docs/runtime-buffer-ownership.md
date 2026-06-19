# Runtime Buffer Ownership

`RuntimeBufferHandle` is the bridge protocol for large payloads. The native
transport passes handles across the boundary; callers borrow bytes through
`get_buffer` and return the lease with `release_buffer`.

## Rules

- Buffers are created inside the bridge layer, not core coordination crates.
- A handle must have an active lease before `get_buffer` can read it.
- Every leased handle passed across the bridge must be released exactly once.
- `release_buffer` removes the buffer when the final lease is returned.
- Double release and unleased reads fail loudly with typed `CoreError`s.
- Bridge tests must call leak checks for mock/native paths that create runtime
  buffers.

## Wake Payloads

The real wake transport uses handles for large values:

- `BrainWakeRequest.body_state`
- `BrainWakeRequest.system_prompt`
- `BrainWakeRequest.role_assembly`

The TypeScript bridge facade is responsible for hydrating those handles into
the parsed `BrainWakeInput` consumed by the brain island, then releasing every
handle it received.

## Boundary

`napi-rs` glue belongs in `crates/bridge/core-bridge-node`. The shared buffer
store lives in `crates/bridge/core-bridge-api` so native, mock, and future CLI
transports enforce the same ownership protocol without leaking native transport
dependencies into core crates.
