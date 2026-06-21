# Den Assignment Evidence E2E Proof

Status: E2E proof for task 3056

Date: 2026-06-20

Related docs:

- `den-assignment-completion-evidence-loop-plan`
- `den-product-data-ingress-boundary`
- `den-completion-evidence-projection`
- `den-router-metadata-diagnostics`

## Purpose

This proof demonstrates the first Den assignment/completion/evidence loop
without giving Den or the adapter runtime authority.

## Flow

`npm run smoke:den-assignment-evidence-e2e` proves:

1. A Rust session is created through the native bridge.
2. A Den assignment ref enters as observe-only product data via
   `ingestDenProductReference()`.
3. An adapter-side `claim` attempt is denied and does not inject another Rust
   event.
4. Router metadata is recorded in an instance-owned store with sanitized
   provenance.
5. A Rust-owned `deliver_completion` action persists a completion packet.
6. Completion/evidence projection first fails with degraded dispatch.
7. The runtime completion packet remains persisted despite projection failure.
8. A later projection succeeds with Den assignment `work_ref` and runtime
   completion `result_ref`.
9. No worker-pool run is created or required.

## Boundary Assertions

The smoke asserts:

- Den product data enters as `DenDataUpdate`, not as a scheduler queue.
- Adapter lifecycle operations such as claim are denied.
- Completion is produced by Rust action acceptance, not inferred from Den prose.
- Projection failure does not roll back runtime completion.
- Router metadata readback is scoped and redacted.
- Worker pools are not involved.
