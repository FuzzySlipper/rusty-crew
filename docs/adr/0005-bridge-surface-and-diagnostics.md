# 0005. Classify Bridge Setup and Diagnostic Surfaces

Date: 2026-06-20

## Status

Accepted for v1 scaffold cleanup.

## Context

The active bridge manifest defines the stable Rust/TypeScript protocol surface,
but the current napi bridge is still hand-written. During the wake-path proof,
the hand-written layer grew useful methods that are not all 1:1 manifest
operations:

- startup/setup surfaces such as `createSession`;
- internal routing convenience such as `routeAgentMessage`;
- runtime-local helpers such as `registerBrainRuntime`,
  `buildBrainWakeRequestForSession`, and bounded subscription drains;
- diagnostics such as body-state JSON projection, raw action JSON submission,
  row counting, and buffer leak assertions.

The risk is not that these methods exist. The risk is that diagnostic helpers
quietly become the production coordination API.

## Decision

For v1, `bridge-manifest.toml` is the stable protocol specification, not yet a
generated-binding source of truth. The `core-bridge-codegen` crate remains a
placeholder. Hand-written bindings may expose extra methods only when each
method is explicitly classified.

Bridge surfaces fall into four buckets:

1. **Stable manifest operations.** These are the normal production protocol:
   engine lifecycle, brain registration, event/action submission, adapter
   registration, event injection, subscription handles, and runtime buffer
   borrow/release.

2. **Setup/config operations.** These are real engine operations used to
   bootstrap or trigger the runtime, but they are not the brain wake loop
   itself. `createSession` is startup/config bootstrap. `routeAgentMessage` is
   internal agent-to-agent routing and now runs scheduler evaluation.

3. **Runtime-local bridge helpers.** These are not standalone coordination
   authority. They adapt the stable manifest into a practical in-process Node
   runtime. `registerBrainRuntime` binds a TS callback to a registered brain
   handle. `buildBrainWakeRequestForSession` projects body state in Rust and
   builds the three runtime buffers for a wake. Bounded subscription drains are
   the v1 delivery shape for subscription handles.

4. **Diagnostics.** These are test/dev/inspection helpers and must be named or
   documented as such. Examples: `diagnosticProjectBodyStateJson`,
   `diagnosticSubmitBrainActionsJson`, `diagnosticCountRows`, and buffer leak
   assertions.

## Consequences

`createSession` and `routeAgentMessage` are not wake-loop diagnostic bypasses.
They can appear in the `2838` proof as setup/trigger surfaces. Session
bootstrap still needs a later startup-config design, but it is not a blocker
for the wake-path proof.

`projectBodyStateJson` and raw `submitBrainActionsJson` remain available as
deprecated compatibility aliases for existing smokes, but new code should use
the diagnostic-prefixed names. Production wake code should use
`buildBrainWakeRequestForSession`, `wakeBrain`, `submitBrainEvent`, and
`submitBrainActions`.

The Rust-side `NativeBridge::wake_brain` method is validation-only in the
current hand-written bridge. The executable TS runtime owns callback invocation
through `BrainWakeExecutor`, matching ADR 0004. This should stay documented
until the manifest/codegen story is mature enough to remove the confusing
internal Rust method.

If future work decides that `bridge-manifest.toml` must generate every binding,
that becomes a codegen milestone. Until then, drift is acceptable only when
the extra methods are classified as setup, runtime-local, or diagnostic.
