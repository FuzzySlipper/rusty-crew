# Buffered Brain Run Registry Ownership

Status: ownership decision for task 4645

Date: 2026-07-07

## Context

Rusty Crew now has two Rust brain modules that stream through native bridge
buffered runs:

- OpenAI Responses in `crates/bridge/core-bridge-node/src/responses.rs`;
- Rust pi-agent in `crates/bridge/core-bridge-node/src/pi_agent.rs`.

Both paths currently store active wake state in process-global `OnceLock`
registries inside the native bridge crate. The shared state shape lives in
`crates/bridge/core-bridge-node/src/buffered_tools.rs`.

That state is not a trivial transport cache. Each active buffered run owns:

- queued `BrainWakeStreamItem`s waiting to be drained by TypeScript;
- pending neutral tool requests waiting for TypeScript execution;
- submitted tool outputs waiting for the Rust provider loop;
- cancellation status and terminal/error status;
- wake timeout checks while waiting for model/tool progress;
- provider state, transport metrics, and credential secret updates.

TypeScript currently owns the drain loop and tool execution policy in
`ts/packages/brain-island/src/brain-module.ts`, while the native bridge process
global owns the in-flight run state that loop talks to.

## Decision

Buffered brain run lifecycle state should not remain bridge-local authority.
The native bridge may expose transport operations, but the run registry should
move toward an explicit Rust brain-run host owned by the service/runtime
composition boundary.

The target is not to put coordination internals inside brain crates. The
run-host boundary is still below Rusty Crew coordination: it holds per-wake
provider-loop/tool-round state for a Rust brain module, not sessions, queues,
bus routing, profile policy, or persistence.

## Target Ownership

Rust brain/runtime host owns:

- active buffered run records keyed by wake id and brain module id;
- pending tool request queueing;
- submitted tool output delivery;
- cancellation and terminal/error transitions;
- per-run timeout bookkeeping;
- active-run diagnostics;
- shutdown cleanup of nonterminal buffered runs.

Native bridge owns:

- napi argument/result conversion;
- mapping host-handle operations to Rust run-host calls;
- spawning provider-loop worker threads while this architecture remains
  thread-based.

TypeScript owns during this transition:

- selecting model-callable tools from profile/tool policy;
- executing local/MCP/tool bindings;
- polling/draining native stream chunks;
- submitting tool outputs back to Rust;
- recording provider/tool debug references for Rusty View.

## Staged Migration

1. Move the generic buffered run data structures and state transitions out of
   `core-bridge-node` into a non-napi Rust crate under `crates/brains/`.
   Keep behavior identical and prove both brain modules can share it.
2. Add an explicit brain-run host handle/scope at the native bridge boundary.
   Start/drain/submit/cancel operations should target that host instead of a
   process-global `OnceLock`.
3. Thread one service-owned host through brain-island/service-host startup.
   The TypeScript wrapper can hide the handle, but ownership should be per
   service instance, not global process state.
4. Add diagnostics and shutdown cleanup for active buffered runs so a stopped
   service cannot leave invisible in-flight state behind.
5. Delete the bridge-global registries once the handle-scoped path covers both
   OpenAI Responses and Rust pi-agent smokes.

## Guardrails

- Do not make Rust brain crates depend on `core-engine`, `core-session`,
  `core-bus`, `core-body`, `core-persistence`, service-host, or TypeScript
  packages.
- Do not reintroduce separate responses-specific and pi-agent-specific run
  state machines. Module-specific output fields are okay; lifecycle semantics
  should stay shared.
- Do not preserve a hidden bridge-global fallback after the explicit host path
  lands.
- Keep tool implementation execution in TypeScript until a separate task moves
  executable tool bindings. The run host only brokers neutral tool calls and
  outputs.

## Validation

Keep these gates green during each slice:

```sh
cargo test --workspace
npm run smoke:rust-crate-boundaries
npm run smoke:openai-responses-tool-bridge -w @rusty-crew/brain-island
npm run smoke:pi-agent-rust-bridge -w @rusty-crew/brain-island
npm run smoke:bridge-validation
npm run smoke:bridge-native-surface
npm run smoke:bridge-fingerprint-drift
```

After handle-scoped ownership lands, add a regression smoke that starts two
service/host scopes in one Node process with the same wake id and proves their
pending tool requests, submitted outputs, cancellation, and terminal cleanup do
not cross.
