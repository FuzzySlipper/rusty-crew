# pi-crew core bridge manifest (draft v0)

> **Status:** Living PRD for the Rust-core rewrite. The 4 functions + 1 class
> currently imported from upstream `@earendil-works/pi-*` packages are the
> target; everything else that ships today is "pulling concepts" and is a
> candidate for replacement.

This is the single source of truth for the FFI surface between the Rust core
and the TypeScript front-end. Every operation on this list is a *verb on a
handle*; all errors are typed; all payloads are typed; the manifest is the
source of truth, the codegen produces the boilerplate, and humans write the
semantic operation bodies.

It is deliberately small. Asha's `runtime-bridge-api/bridge-manifest.toml`
ships 19 operations across the entire engine boundary; we should not need
more than 12 for the pi-crew core. Adding an operation is a boundary change
and warrants an ADR.

## Provenance

- **Existing system to model:** `/home/dev/pi-crew/pi-service/src/workers/`
  (the worker pool, runtime, supervisor, role assembly, packet poster,
  context status, drain mode, checkpoint) and `/home/dev/pi-crew/pi-core/src/`
  (the typed vocabulary: events, types, errors, security, hooks).
- **Upstream we're escaping from:** `@earendil-works/pi-agent-core` (the
  `Agent` class, agent-loop, types) and `@earendil-works/pi-ai`
  (`streamSimple`, `getModels`, `getProviders`, `createAssistantMessageEventStream`).
  See `pi-crew-upstream-audit.md` and `pi-crew-extraction-analysis.md` for
  the dependency shape; ~18 KLOC of vendored upstream is the realistic
  floor, and the Rust rewrite aims to drive that toward 0.
- **Structural model:** asha's `engine-rs/crates/bridge/runtime-bridge-api/`
  with its `bridge-manifest.toml`, `MANIFEST-FORMAT.md`, and the
  `harness/codegen/bridge-emit.py` emitter. Same idea, different surface.

## Lane philosophy

> **Rust owns authority. TypeScript owns expression and projection. Generated
> contracts define the border.**

- Rust is authoritative: the worker pool state machine, the packet protocol,
  the model selection (catalog → model spec), the policy hooks, the
  session/persistence lifecycle, the retry/backoff, the drain-mode logic.
- TypeScript proposes: picks a model from the catalog, shapes the role
  assembly (system prompt + initial messages + tool set selection), registers
  custom tools, registers Den Channels handlers, formats completion packets
  for the front-end UI.
- TypeScript **never mutates** authoritative state. Rust validates every
  worker claim, every packet post, every tool invocation. The TS side
  cannot fabricate a `CompletionPacket` and call it valid — the Rust side
  re-validates the shape on receipt.
- The manifest is generated into both directions: Rust types in
  `core-bridge-api`, TypeScript types in `ts/packages/contracts`. Hand-editing
  the generated files is forbidden; run `cargo run -p core-bridge-codegen`
  after editing the manifest.

## Boundary discipline

The Rust core **does not**:

- Depend on `napi`, `napi-rs`, `wasm-bindgen`, or any other FFI transport
  library. Those live in the `core-bridge-node` / `core-bridge-cli` crates.
- Expose raw pointers or `*const c_char` in any structured payload.
- Accept `serde_json::Value`, `Box<dyn Any>`, or any dynamic dispatch on
  operation names. Operations are statically dispatched by name at the
  transport layer; the manifest is the dispatch table.
- Cross into the LLM provider surface. `streamSimple` and the provider
  registry stay in the TS front-end (see "LLM boundary" below).

The Rust core **does**:

- Own all opaque handle types — `WorkerHandle`, `SessionHandle`,
  `SubscriptionHandle`, `ToolHandle` — as newtypes. The TS side cannot
  forge a handle.
- Return errors through a single typed error enum
  (`CoreBridgeError`) with a fixed variant set. No string/JSON error blobs.
- Marshal large payloads (system prompts, message history) as
  `RuntimeBufferHandle`s, not inline bytes.
- Validate every input that crosses the boundary. A malformed operation
  call is a `CoreBridgeError::InvalidInput`, not a panic.

## Handle taxonomy

| Handle | Owns | Lifetime |
|--------|------|----------|
| `EngineHandle` | The whole pi-crew runtime instance. One per process. | Process lifetime; created in `initialize_engine`. |
| `WorkerHandle` | One supervised worker pool slot. Many per engine. | Bounded by `spawn_worker` → `abort_worker` / natural completion. |
| `SessionHandle` | One logical session (worker session, full-agent session, delegated child). Many per worker. | Created when the session record is created; archived when the session is archived. |
| `ToolHandle` | One tool registration on a session. Many per session. | Created on `register_tool`; revoked on `revoke_tool` or session archive. |
| `SubscriptionHandle` | One event subscription. Many per process. | Created on `subscribe_events`; cancelled on `unsubscribe_events` or handle drop. |
| `RuntimeBufferHandle` | One reference-counted buffer for large payloads (system prompts, message batches, denormalized snapshots). Many per process. | Reference-counted; released on `release_buffer` or last drop. |

No raw `StateStore` / `SessionStore` / `WorkerPool` handle ever crosses
the boundary. The TS side operates on opaque handles and the core routes
them to the right internal state.

## Error channel

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreBridgeError {
    pub kind: CoreBridgeErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreBridgeErrorKind {
    InvalidInput,            // Manifest drift / type mismatch / shape violation
    NotFound,                // Unknown handle, unknown tool name, etc.
    AlreadyExists,           // Duplicate handle creation
    PolicyDenied,            // Pre-tool-call hook returned block: true
    ToolCallFailed,          // Underlying tool handler threw
    SessionExpired,          // Session was archived or aborted
    WorkerStuck,             // Watchdog triggered
    TimeoutExpired,          // Assignment / idle / response timeout
    ModelStreamRetriesExhausted,
    ModelStreamPartialFailure,
    PersistenceFailure,      // SQLite I/O error, schema mismatch
    CompletionPostRejected,  // Den Core returned not-accepted
    InternalError,           // Bug / invariant violation / oops
}
```

Every operation returns `Result<T, CoreBridgeError>` on the Rust side; the
TS side sees a discriminated union. The `message` is for humans; the
`kind` is for the orchestrator.

## Operations (the FFI surface)

The operations are organized by lifecycle phase. Each is a verb on a handle;
the manifest owns the dispatch table and the codegen produces the napi-rs
wrappers, the mock transport, and the TS client.

### Engine lifecycle

#### `initialize_engine`

- **Surface:** stable
- **Input:** `protocol_runtime::EngineConfig`
  - `engine_data_dir: string` — where to put SQLite, logs, snapshots
  - `clock: "system" | { fixed: "2026-06-19T00:00:00Z" }` — for deterministic tests
  - `default_turn_budget: u32`
  - `default_idle_timeout_ms: u32`
  - `default_assignment_timeout_ms: u32`
- **Output:** `EngineHandle`
- **Errors:** `InvalidInput`, `PersistenceFailure`, `InternalError`
- **Summary:** Construct a pi-crew engine instance. Reads the runtime
  config, opens the SQLite store, hydrates any persisted session records
  (best-effort, fail-soft). Returns an opaque handle, never a state store.
- **TS equivalent of today:** implicit in `WorkerRuntime` constructor +
  `SessionManager.create` + DB open. Today this happens lazily across
  many files; the manifest makes it one verb.

#### `shutdown_engine`

- **Surface:** stable
- **Input:** `{ engine: EngineHandle, drain_timeout_ms: u32 }`
- **Output:** `{ archived_sessions: u32, aborted_workers: u32 }`
- **Errors:** `InternalError`
- **Summary:** Drain all workers, archive all sessions, flush
  SQLite, close the engine. Idempotent on the second call.

### Worker lifecycle

#### `spawn_worker`

- **Surface:** stable
- **Input:** `WorkerSpawnRequest`
  - `worker_id: WorkerId` (caller-chosen; core validates uniqueness)
  - `session_id: SessionId` (the session this worker belongs to)
  - `role: "coder" | "reviewer" | "validator" | "packet_auditor" | ...`
  - `role_assembly: RuntimeBufferHandle` (a buffer holding the serialized
    `WorkerRoleAssembly` JSON: `system_prompt`, `initial_messages[]`,
    `mcp_tool_sets[]`, `drain_essential_tools[]`, optional `extra_hooks`
    descriptor)
  - `model: ModelSpec` (see protocol section below — provider, model name,
    base URL, temperature, max tokens, API key, stream-retry config)
  - `policy: WorkerPolicy` (allowed tools, denied tools, allowed hosts,
    denied hosts, workdir, max duration, idle timeout, max delegation depth)
  - `binding: WorkerBinding` (Den assignment ID / run ID / task ID / project ID)
- **Output:** `WorkerHandle`
- **Errors:** `InvalidInput`, `AlreadyExists`, `SessionExpired`, `NotFound`,
  `PolicyDenied`, `PersistenceFailure`, `InternalError`
- **Summary:** Claim a worker slot for the given session. Validates the role
  assembly, the model spec, and the policy. Persists a worker run record
  with status `claimed`. Returns an opaque `WorkerHandle`. The model
  stream is *not* started — that's `prompt_worker`.
- **TS equivalent of today:** `WorkerRuntime.executeAssignment` lines
  141-209 (claim → session → context build) and `AgentWorkerFactory.create`
  (lines 102-125) rolled into one verb with a typed input envelope.

#### `prompt_worker`

- **Surface:** stable
- **Input:** `{ worker: WorkerHandle, messages: RuntimeBufferHandle }` —
  the buffer holds the initial `AgentMessage[]` payload
- **Output:** `{ accepted: true }`
- **Errors:** `InvalidInput`, `NotFound`, `SessionExpired`, `TimeoutExpired`,
  `ModelStreamRetriesExhausted`, `PolicyDenied`, `InternalError`
- **Summary:** Hand the worker its initial message batch and start the
  model stream. Returns immediately after the first token arrives (or the
  first event is published). The rest of the run emits events on the
  subscription registered via `subscribe_events`.
- **TS equivalent of today:** `agent.prompt(assembly.buildInitialMessages(roleInput))`
  followed by `agent.waitForIdle()` (lines 258-259 of
  `agent-worker-executor.ts`). The "wait for idle" part is gone — events
  carry the lifecycle now.

#### `abort_worker`

- **Surface:** stable
- **Input:** `{ worker: WorkerHandle, reason: "human" | "timeout" | "policy" | "engine_shutdown" }`
- **Output:** `{ final_state: WorkerFinalState }` — see below
- **Errors:** `NotFound`, `InternalError`
- **Summary:** Cancel the worker's current model stream and tool execution
  loop, fire the abort event, transition to `aborted` state. Idempotent.
- **TS equivalent of today:** `agent.abort()` (line 252) plus
  `context.signal?.abort` propagation. The `WorkerExecutionContext` signal
  pattern collapses into a typed reason field.

#### `await_worker_completion`

- **Surface:** stable
- **Input:** `{ worker: WorkerHandle, timeout_ms: u32 }`
- **Output:** `WorkerFinalState`
  ```rust
  pub enum WorkerFinalState {
      Completed { packet: CompletionPacket },
      Failed { packet: CompletionPacket },
      Blocked { packet: CompletionPacket },
      Exhausted { packet: CompletionPacket },
      Aborted { reason: String, partial_packet: Option<CompletionPacket> },
      Timeout { last_known_state: WorkerRuntimeState },
  }
  ```
- **Errors:** `NotFound`, `InternalError`
- **Summary:** Block until the worker reaches a terminal state or the
  timeout elapses. This is the join-point for orchestrators. Returns the
  final packet in all but the timeout case.
- **TS equivalent of today:** the result of `executeWithAssignmentTimeout`
  in `worker-runtime.ts` line 167-183, plus the `CompletionPacket` build at
  line 185-208. Today the packet is built even on timeout; the manifest
  preserves that but with explicit `Timeout` variant for clarity.

#### `register_tool` / `revoke_tool`

- **Surface:** stable
- **Input (register):** `{ session: SessionHandle, tool: ToolDefinition }`
  - `name: string` — tool name, must be unique within the session
  - `description: string` — shown to the agent as the tool docstring
  - `parameters: JsonSchema` — JSON Schema for the tool's input
  - `execute: ToolExecutorDescriptor` — discriminated union:
    - `{ kind: "wasm", module: RuntimeBufferHandle, function: string }`
    - `{ kind: "ts_builtin", function: string }` (registered out-of-band
      by the TS front-end, looked up by name on the Rust side)
    - `{ kind: "delegated_spawn", config: DelegatedSpawnConfig }` —
      spawns a child worker; the descriptor is the same shape as the
      one in `delegated-spawn-tool.ts` today
- **Output (register):** `ToolHandle`
- **Errors:** `InvalidInput`, `AlreadyExists`, `NotFound`, `SessionExpired`
- **Summary (register):** Add a tool to a session's active tool surface.
  Tools registered this way are *not* subject to drain-mode removal unless
  they appear in the role assembly's `drain_essential_tools` list. Tools
  registered during drain mode are rejected with `SessionExpired`.
- **Summary (revoke):** Remove a tool. Idempotent.

#### `get_worker_state`

- **Surface:** stable
- **Input:** `{ worker: WorkerHandle }`
- **Output:** `WorkerRuntimeState`
  ```rust
  pub struct WorkerRuntimeState {
      pub phase: "claimed" | "running" | "draining" | "checkpoint_waiting" | "completed" | "failed" | "aborted",
      pub turn_count: u32,
      pub tokens_used: u32,
      pub tools_active: Vec<String>,
      pub drain_active: bool,
      pub last_event_at: IsoTimestamp,
  }
  ```
- **Errors:** `NotFound`
- **Summary:** Snapshot the worker's current state. Cheap, no I/O.
- **TS equivalent of today:** `supervisor.turnCount` + `supervisor.tokensUsed`
  + `checkpointController.phase` + `drainModeManager.active` rolled into
  one query. Today these are four separate getters across three objects;
  the manifest gives the orchestrator one read.

### Session lifecycle

#### `create_session` / `archive_session` / `get_session_state`

- **Surface:** stable
- **Input (create):** `SessionConfig`
  - `session_id: SessionId` (caller-chosen, validated unique)
  - `profile_id: ProfileId`
  - `kind: "full" | "worker" | "delegated"`
  - `delegation?: DelegationLineage`
  - `effective_runtime?: EffectiveDelegationRuntime`
  - `worker_binding?: WorkerBinding` (worker kind only)
  - `channel_bindings?: ChannelBindingRecord[]` (full kind only)
- **Output (create):** `SessionHandle`
- **Errors:** `InvalidInput`, `AlreadyExists`, `PersistenceFailure`
- **Summary (create):** Open a session record in the runtime store. Returns
  an opaque handle. Does *not* spawn a worker; that's `spawn_worker`.
- **Summary (archive):** Close a session, release its tools, cancel its
  subscriptions. Idempotent.
- **Summary (get):** Returns a `SessionState` summary including
  `state: "active" | "idle" | "archived"`, `instance_id`, `message_count`,
  `created_at`, `last_active_at`, plus the `WorkerBinding` (if any) and
  the list of currently active `ToolHandle`s.

### Events

#### `subscribe_events` / `unsubscribe_events`

- **Surface:** stable
- **Input (subscribe):** `EventSubscription`
  - `event_kinds: Vec<GatewayEventKind>` — empty list means "all events"
  - `filter?: { assignment_id?: AssignmentId, session_id?: SessionId, worker_id?: WorkerId }`
- **Output (subscribe):** `SubscriptionHandle`
- **Errors:** `InvalidInput`, `NotFound`
- **Summary (subscribe):** Register an event sink. The transport layer
  (napi-rs / CLI / mock) handles the actual delivery; on napi-rs this
  becomes an `EventEmitter` subscription, on CLI it writes to stdout, on
  mock it pushes into a `Vec<Event>` for tests. The event vocabulary is
  the same 44-kind `GatewayEvent` union from `pi-core/src/events.ts`,
  unchanged.
- **Summary (unsubscribe):** Cancel a subscription. Idempotent.

### Catalogs

#### `register_model_catalog`

- **Surface:** stable
- **Input:** `ModelCatalog`
  ```rust
  pub struct ModelCatalog {
      pub models: Vec<ModelSpec>,
      // ModelSpec is the same shape the orchestrator needs to know
      // about a model: provider, model name, base URL, context window,
      // max tokens, capability flags, cost. NOT the upstream
      // `Model<Api>` shape — the Rust core doesn't know or care
      // about provider APIs.
  }
  ```
- **Output:** `{ accepted: u32, rejected: Vec<{ index: u32, reason: String }> }`
- **Errors:** `InvalidInput`
- **Summary:** Hand the core a list of available models. The core
  validates each spec (context window > 0, max tokens > 0, provider
  non-empty, base URL parseable if present) and rejects the bad ones
  with a typed reason. This is the *front end of model selection* — the
  catalog is policy, the core just stores it. Today this happens
  implicitly through `getModels(provider).find((m) => m.id === ...)` in
  `agent-worker-executor.ts:393`; the manifest makes it explicit.
- **Why this matters:** the catalog never crosses back across the
  boundary at request time. When `spawn_worker` references a model by
  `(provider, model_name)`, the core looks it up in its own catalog and
  fails with `NotFound` if it's missing. The TS side does not have to
  pass the full spec at spawn time; it passes the spec, and the core
  validates and stores it.

#### `resolve_model`

- **Surface:** stable
- **Input:** `{ provider: string, model_name: string }`
- **Output:** `ModelSpec` (the stored spec, not the upstream `Model<Api>`)
- **Errors:** `NotFound`
- **Summary:** Look up a model by `(provider, name)`. Used by the TS
  front-end to verify a model exists before passing it to `spawn_worker`.
  Cheap, no I/O.

## LLM boundary

The Rust core **does not call any LLM provider API.** `streamSimple` and
the provider registry stay in the TS front-end; the TS side owns the
`Agent` class equivalent (or a replacement) and pushes events into the
core via `subscribe_events` and the worker lifecycle verbs.

The shape is: the core's `WorkerHandle` corresponds to *the supervised
worker*, not to the upstream `Agent`. The supervised worker has a
`prompt_worker` and an event stream; the TS side is responsible for
turning those into actual LLM API calls and forwarding the events back
through the subscription.

Today this is the opposite: the `Agent` runs in the core, and the TS
side calls `getModels`/`getProviders` from upstream to set up the call.
The rewrite inverts it. The reason is that the LLM provider surface is
the largest moving part (3rd-party API churn, vendor-specific options,
streaming quirks) and the TS side already has working code for it
(`@earendil-works/pi-ai`). Putting it behind the FFI gives the Rust
core a clean abstraction: *workers produce events; events are typed;
the LLM is a TS-side concern.*

This is a non-trivial decision and warrants an ADR. The trade is:

- **Pro:** Rust core doesn't need to know about Anthropic / OpenAI /
  Bedrock / etc. Smaller surface, easier to vendor upstream, no
  provider SDK maintenance.
- **Con:** the LLM stream crosses the FFI per event. Either the events
  are batched (lossy) or the FFI is high-frequency (cost in napi-rs
  marshalling). Mitigation: events are small (text deltas, usage
  counters, tool call markers) and the FFI can batch them at the
  transport layer.

Alternative: put the LLM surface in Rust by vendoring upstream's
`stream.ts` and `models.generated.ts` (Tier 2 of the extraction
analysis, ~18 KLOC + 25 external SDKs). This is more work but keeps
the hot path off the FFI. Worth a separate decision; this manifest
assumes the first option (TS-side LLM) for now and notes the
alternative.

## Protocol types (codegen output)

The codegen produces these from the manifest. The TS side imports them
from `ts/packages/contracts`; the Rust side defines them in
`core-bridge-api/src/protocol/`. Both are generated; humans do not
hand-edit.

| Type | Source of truth | Notes |
|------|-----------------|-------|
| `EngineHandle` | manifest handle taxonomy | newtype around `u64` |
| `WorkerHandle` | manifest handle taxonomy | newtype around `u64` |
| `SessionHandle` | manifest handle taxonomy | newtype around `u64` |
| `ToolHandle` | manifest handle taxonomy | newtype around `u64` |
| `SubscriptionHandle` | manifest handle taxonomy | newtype around `u64` |
| `RuntimeBufferHandle` | manifest handle taxonomy | newtype around `u64` |
| `CoreBridgeError` | manifest error channel | enum + message |
| `CoreBridgeErrorKind` | manifest error channel | fixed variant set |
| `EngineConfig` | `initialize_engine` input | |
| `WorkerSpawnRequest` | `spawn_worker` input | includes `ModelSpec`, `WorkerPolicy`, `WorkerRoleAssembly` |
| `WorkerFinalState` | `await_worker_completion` output | |
| `WorkerRuntimeState` | `get_worker_state` output | |
| `SessionConfig` | `create_session` input | |
| `SessionState` | `get_session_state` output | |
| `ToolDefinition` | `register_tool` input | includes `ToolExecutorDescriptor` |
| `ModelCatalog` | `register_model_catalog` input | |
| `ModelSpec` | catalog + spawn | provider, name, base URL, context window, max tokens, capabilities, cost |
| `WorkerPolicy` | spawn | mirrors `WorkerPolicy` in `pi-core/src/security.ts` |
| `WorkerRoleAssembly` | spawn | system prompt, initial messages, MCP tool sets, drain essentials, extra hooks descriptor |
| `WorkerBinding` | spawn | mirrors `WorkerBinding` in `pi-service/src/sessions/types.ts` |
| `CompletionPacket` | output | unchanged from `pi-core/src/types.ts` |
| `CompletionArtifact` | output | unchanged |
| `CompletionBlocker` | output | unchanged |
| `CompletionStatus` | output | unchanged enum: `completed` / `failed` / `blocked` / `exhausted` |
| `GatewayEvent` | events | unchanged from `pi-core/src/events.ts` (44 kinds) |
| `GatewayEventKind` | events | string enum |
| `EventSubscription` | `subscribe_events` input | |
| `IsoTimestamp` | various | `string` newtype |
| `WorkerId` / `SessionId` / `AssignmentId` / `TaskId` / `RunId` / `ProjectId` / `ProfileId` | various | newtypes around `string` / `u64` |

## What this manifest does NOT cover

By design, the manifest is small. The following live in the TS front-end
and are not part of the FFI:

- **LLM provider selection** — `streamSimple`, `getModels`, `getProviders`,
  the `Model<Api>` shape. The TS side owns the catalog; the core owns the
  spec store.
- **MCP server implementation** — the core's `register_tool` accepts a
  `ToolExecutorDescriptor` that points to a TS-side builtin; the TS side
  talks to MCP servers. Today this is `pi-mcp/src/`.
- **Den Channels adapter** — `pi-channels/src/den-channels-adapter.ts`
  and the `telegram-channel-provider.ts`. These are TS-side.
- **TUI / CLI** — `pi-crew/src/debug-tui.ts`, `pi-crew/src/debug-cli.ts`,
  `pi-crew/src/cron-cli.ts`. TS-side.
- **The local code tools** — `pi-tools/src/local-code-tools.ts` and the
  individual tool implementations (`patch-tool`, `todo-tool`, etc.). TS-side.
- **The profile system** — `pi-profiles/src/loader.ts`,
  `pi-profiles/src/system-prompt.ts`. TS-side; the core receives a
  resolved `WorkerRoleAssembly` buffer.

This separation is the whole point of the rewrite: the Rust core owns
*authority over worker lifecycle, packet protocol, and policy enforcement*,
and the TS front-end owns *expression, projection, and integration with
the rest of the Den stack*.

## Open questions for the implementation agent

1. **Where does the `Agent` class equivalent live?** The current design
   has the core own the `WorkerHandle` and the TS side own the LLM
   call. The TS side therefore needs its own `Agent` equivalent — either
   a thin re-export of `@earendil-works/pi-agent-core`'s `Agent` (cheapest
   but keeps the upstream dep) or a from-scratch rewrite (most work,
   biggest win). Decision should be in an ADR.

2. **Tool executor descriptors.** The `register_tool` `ToolExecutorDescriptor`
   is currently a 3-variant enum. The `wasm` variant is aspirational —
   today there is no WASM tool path. Should we ship with just
   `ts_builtin` and `delegated_spawn`, and add `wasm` later? Or design
   the descriptor so the `wasm` variant is a forward-compatible no-op?

3. **Buffer ownership.** `RuntimeBufferHandle`s are reference-counted and
   can be passed across the FFI (e.g., "here's the system prompt as a
   buffer"). The reference-count protocol needs to be airtight: every
   buffer passed in must be released, every buffer received must be
   released. The napi-rs transport should use a `Drop` impl that
   decrements the count; on the CLI transport, the mock should
   fail-loud on leaks in tests. Worth a separate doc.

4. **The 25-event kinds in `pi-core/events.ts` that aren't worker
   lifecycle.** Today: `session.created`, `session.routing`,
   `session.expired`, `session.reset`, `mcp.reload.*`, `blackboard.written`,
   `gateway.shutdown`, `admin_control.*`, `delegation.*`, etc. These are
   emitted by the TS front-end (channels, MCP reload, admin), not by
   the core. The manifest's event subscription covers them anyway because
   the core owns the dispatcher; the TS-side emitters push into the
   core via a separate `emit_event` operation. Should this operation be
   on the manifest, or is it an implementation detail of the TS side?

5. **Replay determinism.** Asha has it (frame-accurate replay of the
   simulation). The pi-crew rewrite would benefit — packet-auditor and
   validator roles can replay an LLM run exactly the same way. But
   it's a significant feature and would change the `WorkerFinalState`
   shape (add a `ReplayHandle`). Note as a future-extension, do not
   block the initial design on it.

## Anti-patterns this manifest is designed to prevent

1. **The orchestrator wrapping the worker's stream at construction time.**
   Today: `agent-worker-executor.ts:104-116` wraps `streamSimple` with
   retry, replaces the API key resolver, and injects a message transform
   — all at `Agent` construction. The result is that the worker's
   "agent behavior" is replaced by the harness at runtime, with the
   hook registry, the policy hooks, and the message transforms all
   competing for the same surface. The manifest gives each of these
   a typed operation (`register_tool`, `subscribe_events`, the
   `WorkerRoleAssembly.extra_hooks` descriptor) so they cannot
   collide on the Agent constructor.

2. **The supervisor re-mapping the same event twice.** Today:
   `agent-supervisor.ts` subscribes to `agent.subscribe(event)` and
   maps 7 Agent event types to typed GatewayEvents. With the manifest,
   the LLM events come through `subscribe_events` *as* the typed
   GatewayEvents (because the TS-side `Agent` is responsible for
   emitting them, not the core). The supervisor's job shrinks to
   *correlating* events with assignments, not *translating* them.

3. **The `beforeToolCall` / `afterToolCall` hooks as escape hatches.**
   Today: `guarded-tool-types.ts` re-declares the upstream hook types
   as structural mirrors specifically to avoid coupling to the
   upstream package. The comment says "without importing from the
   external package directly." The manifest's `register_tool` with
   `ToolExecutorDescriptor` makes the same shape first-class: tools
   are registered, hooks are descriptor metadata, and the
   `before/afterToolCall` surface is owned by the core, not
   borrowed from upstream.

4. **The completion packet's 5 status values + the 4-from-blocker
   matrix.** Today: `CompletionStatus` is `completed | failed | blocked |
   exhausted`, and the `WorkerFinalState` can be any of those plus
   `aborted` plus `timeout`. The manifest keeps the packet shape
   (Den Core's contract) and gives the worker a separate
   `WorkerFinalState` enum that has the timeout/aborted variants
   the orchestrator needs. Two enums, two concerns, no overlap.

5. **The "config struct with 12 optional fields" pattern.** Today:
   `WorkerModelConfig`, `WorkerRoleConfig`, `WorkerExecutionContext`,
   `SessionConfig`, `AgentWorkerFactoryInput`, and
   `AgentWorkerExecutorConfig` are all variations on the same shape
   with overlapping optional fields. The manifest reduces this to
   two: `ModelSpec` (the model + parameters) and `WorkerPolicy`
   (the execution constraints). Everything else is implied by the
   `WorkerRoleAssembly` buffer or the `SessionHandle`.

## Relationship to existing pi-crew docs

- `pi-crew-upstream-audit.md` — the dependency audit that motivated this
  rewrite. The 51 type-only imports + 5 value imports + 4 functions +
  1 class surface in that audit is what this manifest replaces.
- `pi-crew-extraction-analysis.md` — the feasibility study. Tier 2
  (18 KLOC of upstream to vendor) is the floor if we don't rewrite;
  this manifest is the ceiling if we do. The two are mutually
  exclusive: doing the rewrite obviates the vendoring decision.

## Versioning

- **Major version bump** for any change to a `Handle` type, the
  `CoreBridgeErrorKind` variant set, or an `Output` shape that removes
  a field.
- **Minor version bump** for adding a new operation, adding a new
  field to an `Input` or `Output`, or adding a new `CoreBridgeErrorKind`
  variant.
- **Patch version bump** for documentation, codegen improvements, or
  tightening validation in a backwards-compatible way.

The TS `contracts` package version is bumped in lockstep with the
manifest. The `core-bridge-api` Rust crate version is bumped in
lockstep. The codegen step fails CI if the generated files don't
match the manifest — there is no scenario where the two sides are
out of sync.
