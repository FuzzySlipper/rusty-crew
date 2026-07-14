# Codex External Runtime Boundary

This package is the codec and transport boundary for the Codex app-server
external-agent lane. It is deliberately not a brain module and does not own
compatibility policy, lifecycle, retries, scheduling, control idempotency,
identity, or restart decisions.

The shipping transport is WebSocket over the configured Unix socket with
compression disabled. Stdio is a diagnostic compatibility oracle only; there
is no runtime fallback from Unix transport to stdio or TCP.

`protocol/0.144.1` is the committed generation baseline from `codex-cli
0.144.1`, including the experimental API required for `dynamicTools`. It is a
development and regeneration reference, not a production admission pin. The
manifest records launcher, native executable, TypeScript, JSON schema, and
combined protocol fingerprints without recording machine-local paths.

Regenerate when intentionally advancing the committed protocol baseline:

```bash
npm run codegen:codex-app-server
npm run check:codex-app-server-protocol -- --runtime
```

The offline check validates committed artifact hashes without requiring Codex
to be installed. The `--runtime` form regenerates into scratch space and reports
version, executable, or schema drift for the update workflow.

`consumed-contract.ts` names every response and server-request shape Crew
actually consumes. Incoming validators preserve required generated fields and
types while allowing additive object fields, so harmless Codex additions do not
force a Crew release. Missing or malformed consumed fields still fail closed
with stable protocol-fault reason codes.

`CodexAppServerDriver` multiplexes many native threads over one controller
connection and maps native notifications into runtime-neutral callback values.
Every server request is lease-gated and delegated to the required Rust authority
callback. Unknown requests fail closed; unknown notifications remain bounded raw
evidence. Rust remains authoritative for whether the observed contract is
compatible and whether turns may start.
