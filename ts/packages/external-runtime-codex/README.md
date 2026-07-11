# Codex External Runtime Boundary

This package is the exact-version codec and transport boundary for the Codex
app-server external-agent lane. It is deliberately not a brain module and does
not own lifecycle, retries, scheduling, control idempotency, identity, or
restart decisions.

The shipping transport is WebSocket over the configured Unix socket with
compression disabled. Stdio is a diagnostic compatibility oracle only; there
is no runtime fallback from Unix transport to stdio or TCP.

`protocol/0.144.1` is generated from the installed `codex-cli 0.144.1`
executable, including the experimental API required for `dynamicTools`. The
manifest records launcher, native executable, TypeScript, JSON schema, and
combined protocol fingerprints without recording machine-local paths.

Regenerate after an intentional pinned-runtime update:

```bash
npm run codegen:codex-app-server
npm run check:codex-app-server-protocol -- --runtime
```

The offline check validates committed artifact hashes without requiring Codex
to be installed. The `--runtime` form regenerates into scratch space and rejects
version, executable, or schema drift.

`CodexAppServerDriver` validates both directions against the generated schemas,
multiplexes many native threads over one controller connection, and maps native
notifications into runtime-neutral callback values. Every server request is
lease-gated and delegated to the required Rust authority callback. Unknown
requests fail closed; unknown notifications remain bounded raw evidence.
