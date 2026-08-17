# ADR 0025: Managed External Agent Runtime

Status: Accepted

Date: 2026-07-10

Related tasks: `#5515`, `#5517`, `#5518`, `#5610`, `#5739`, `#5783`,
`#5790`

Related evidence:

- `docs/codex-app-server-0.144.1-live-semantics-spike.md`
- `docs/codex-app-server-0.144.3-live-certification.md`
- `docs/codex-debug-update-certification.md`
- `[doc: rusty-crew/codex-app-server-external-agent-lane]`
- `[doc: rusty-crew/responses-brain-codex-translation-plan-2026-06-24]`

## Context

Rusty Crew has direct brain modules whose provider loops implement the neutral
wake contract. Rust freezes body state, starts a wake, receives streamed brain
events, and accepts one terminal action batch or failure. This remains the right
contract for Rust-owned Chat Completions and OpenAI Responses loops.

Codex app-server is a complete agent runtime above that boundary. It owns native
threads, turns, provider calls, command and file execution, MCP, skills, hooks,
approvals, structured questions, compaction, and subagents. Converting those
semantics into `BrainAction` would hide the capabilities being adopted and would
incorrectly transfer Codex execution ownership to Crew.

Task `#5517` proved the installed `codex-cli 0.144.1` runtime over a supervised
Unix WebSocket. A non-ephemeral thread performed a real code edit, ran tests,
used Den MCP, called a Crew dynamic tool, resumed after app-server replacement,
and recovered an in-flight hard-killed turn as interrupted. That evidence
supersedes the earlier managed-stdio-first planning assumption.

Task `#5790` subsequently certified `codex-cli 0.144.3` without regenerating or
editing the `0.144.1` protocol baseline. The consumed contract remained
compatible, the debug workflow passed real provider/tool/restart checks, and a
guarded promotion preserved live Crew binding and native thread identity. This
proves that generated protocol artifacts are a development baseline, not an
operator-edited runtime version pin.

Rusty Crew needs a runtime-neutral architecture that can host Codex now and other
complete external agent runtimes later without making every agent a
`BrainModule`, without making TypeScript lifecycle authority, and without asking
Rusty View to speak a native runtime protocol.

## Decision

Rusty Crew adds a **managed external agent runtime lane** alongside direct brain
modules.

The two lanes share durable Crew session identity, routing, capacity,
observation, and control surfaces. They do not share provider-loop ownership:

```text
AgentMessageRouted
        |
        v
Rust-owned AgentActivation decision
        |
        +-- DirectBrainWakeRequested --> Rust brain runtime --> BrainAction
        |
        `-- ExternalTurnRequested ----> external runtime controller
                                         --> native turn/items
                                         --> normalized Crew events
```

The first external runtime implementation is Codex app-server attached through
the independently supervised Unix WebSocket at a configured endpoint. Managed
stdio is retained only as a diagnostic compatibility oracle, not as a production
fallback. TCP WebSocket and importing Codex source or crates are excluded.

## Independent Axes

`SessionKind` continues to mean `full`, `worker`, or `delegated`. Runtime
selection is an independent binding:

```text
Session lifecycle kind
  full | worker | delegated

Execution binding
  direct_brain { profile_id, implementation_id }
  external_agent { runtime_id, binding_id, native_thread_id? }
```

A full or delegated session may use either execution lane. Worker-pool policy is
not encoded into external-runtime transport. Imported historical Codex threads
may be observable without being routable Crew agents.

## Authority

### Rust owns

- runtime and endpoint registration;
- desired and observed runtime state;
- endpoint ownership mode and exclusive controller lease;
- Crew agent/session/run to native server/thread/turn correlation;
- runtime-neutral activation and message routing decisions;
- turn admission, capacity, waiting, suspension, and queued follow-up policy;
- control idempotency, authorization capabilities, and restart reconciliation;
- pending interaction metadata, expiry, and stale resolution;
- normalized low-rate coordination events and browser replay sequencing;
- durable state, retention, and redaction policy;
- consumed-contract compatibility requirements and admission decisions.

### The external runtime owns

- native thread, turn, item, and transcript truth;
- provider/model loop and context management;
- native commands, file edits, tool calls, MCP, skills, hooks, and subagents;
- native approvals/questions/elicitation request production;
- native compaction and native configuration interpretation.

### TypeScript owns

- Unix WebSocket transport and JSON-RPC framing;
- generated protocol-baseline and consumed-contract codec/validation glue;
- conversion between native protocol messages and manifest-owned bridge values;
- native request response I/O after Rust has selected the policy result;
- bounded raw-message capture after redaction.

TypeScript does not choose retries, synthesize lifecycle state, allocate
capacity, decide message routing, or mutate durable correlation records.

### Rusty View owns

- browser presentation and local UI state;
- calls to Crew's browser-safe APIs;
- rendering normalized transcript, interaction, and attention events.

Rusty View never connects to app-server directly and never races the Crew
controller for server callbacks.

## Neutral Contract

The following shapes are normative in meaning. Implementation tasks may split
them across protocol, engine, persistence, and generated bridge crates without
changing their ownership.

### Runtime registration

```rust
enum ExternalRuntimeKind {
    CodexAppServer,
}

enum ExternalEndpointTransport {
    UnixWebSocket,
}

enum ExternalProcessOwnership {
    Attached, // supervised elsewhere; Crew must not kill it
    Managed,  // reserved for a later explicit supervisor implementation
}

enum ExternalRuntimeCompatibilityState {
    Unassessed,
    CompatibleUncertified,
    Certified,
    Incompatible,
}

struct ExternalRuntimeRegistration {
    runtime_id: ExternalRuntimeId,
    kind: ExternalRuntimeKind,
    endpoint: ExternalEndpoint,
    process_ownership: ExternalProcessOwnership,
    codex_home_ref: Option<String>,
    observed_cli_version: Option<String>,
    consumed_contract_revision: Option<String>,
    compatibility_state: ExternalRuntimeCompatibilityState,
    enabled: bool,
    revision: u64,
}
```

Endpoint records contain non-secret local endpoint identity. Auth material, if a
future runtime requires it, uses the existing secret-envelope boundary and is
never returned by browser APIs.

### Runtime and controller state

```rust
enum ExternalRuntimeDesiredState {
    Enabled,
    Disabled,
}

enum ExternalRuntimeObservedState {
    Disconnected,
    Connecting,
    Ready,
    Degraded,
    Incompatible,
}

struct ExternalControllerLease {
    runtime_id: ExternalRuntimeId,
    holder_instance_id: String,
    generation: u64,
    acquired_at: IsoTimestamp,
    renewed_at: IsoTimestamp,
    expires_at: IsoTimestamp,
    revision: u64,
}
```

Lease acquisition and renewal are compare-and-swap database operations. A
driver may answer native server requests only while it holds the current lease
generation. A stale driver disconnects and may not answer, steer, or interrupt.

### Agent binding

```rust
enum ExternalBindingPurpose {
    CrewAgent,
    ImportedObserver,
}

struct ExternalAgentBinding {
    binding_id: ExternalBindingId,
    runtime_id: ExternalRuntimeId,
    session_id: Option<SessionId>,
    agent_id: Option<AgentId>,
    purpose: ExternalBindingPurpose,
    native_thread_id: Option<String>,
    cwd: Option<String>,
    task_ref: Option<DenRuntimeReference>,
    effective_config_fingerprint: String,
    status: ExternalBindingStatus,
    revision: u64,
}
```

Only `CrewAgent` bindings with active Crew identity and session records are
routable. Discovering or importing a native thread creates no agent identity and
grants no message target by itself.

### Runtime-neutral activation

```rust
enum AgentActivation {
    DirectBrainWakeRequested {
        session_id: SessionId,
        wake_id: WakeId,
    },
    ExternalTurnRequested {
        session_id: SessionId,
        request_id: ExternalTurnRequestId,
        binding_id: ExternalBindingId,
    },
    QueuedForNextTurn {
        session_id: SessionId,
        queue_id: String,
    },
    Rejected {
        reason_code: String,
    },
}
```

The router resolves a durable agent identity to its session and execution
binding, then returns one activation result. It does not call all executable
agents brains.

For an idle external binding, a routed message creates an external turn request.
For an active external turn, ordinary messages enter the Rust-owned next-turn
queue. They never silently become native `turn/steer`. Steering is a distinct,
explicit control with an expected native turn ID.

### Turn request and correlation

```rust
struct SessionTurnRequested {
    request_id: ExternalTurnRequestId,
    idempotency_key: String,
    session_id: SessionId,
    run_id: Option<RunId>,
    binding_id: ExternalBindingId,
    input: ExternalTurnInput,
    provenance: TurnInputProvenance,
    created_at: IsoTimestamp,
    expires_at: Option<IsoTimestamp>,
}

struct ExternalTurnCorrelation {
    request_id: ExternalTurnRequestId,
    runtime_id: ExternalRuntimeId,
    binding_id: ExternalBindingId,
    session_id: SessionId,
    run_id: Option<RunId>,
    native_thread_id: String,
    native_turn_id: Option<String>,
    task_ref: Option<DenRuntimeReference>,
    phase: ExternalTurnPhase,
    capacity_lease_id: Option<String>,
    revision: u64,
}
```

Inputs are typed content parts plus machine provenance. Gate results, direct
agent messages, operator chat, and scheduled wakes remain distinguishable. Crew
does not disguise machine facts as unlabelled user prose.

The input TTL governs admission and dispatch waiting only. Rust terminalizes an
`accepted` request as `failed` with `external_turn_dispatch_expired` when its TTL
elapses before native dispatch. Once the controller has entered `starting`, the
input TTL no longer caps the native turn. A native start request whose outcome
cannot be established becomes `outcome_unknown`; it is never silently replayed.

`ExternalTurnPhase` is one of:

```text
accepted -> starting -> active
active -> waiting_interaction -> active
active -> completed | failed | interrupted
accepted -> failed | interrupted
starting -> failed | interrupted | outcome_unknown
```

Terminal phases are immutable. A native turn ID is bound once. Conflicting
rebinds fail closed.

### Controls

```rust
enum ExternalControlKind {
    StartOrResumeThread,
    StartTurn,
    SteerTurn,
    InterruptTurn,
    CompactThread,
    ExecuteThreadCommand,
    ResolveInteraction,
    ReconcileRuntime,
    ArchiveBinding,
}

struct ExternalControlRequest {
    control_id: String,
    idempotency_key: String,
    binding_id: ExternalBindingId,
    expected_binding_revision: u64,
    expected_native_turn_id: Option<String>,
    kind: ExternalControlKind,
    payload: ExternalControlPayload,
    requested_at: IsoTimestamp,
}
```

Each control has an idempotent receipt. Repeating the same key and payload
returns the same result. Reusing a key with a different payload is a conflict.
Steer and interrupt require the expected active native turn ID. Compact requires
an idle thread unless an observed compatible capability explicitly proves
otherwise.

`ExecuteThreadCommand` carries one allow-listed command name plus an optional
bounded argument. Rust validates the command envelope and persists its receipt;
the generated-protocol TypeScript adapter performs the native protocol call.
The browser command endpoint requires an idempotency key, and recognized command
text is rejected by the ordinary external-message endpoint so it cannot become
native user input accidentally.

### Thread commands and context ownership

Crew exposes a binding-scoped command catalog and execution endpoint for
`/help`, `/commands`, `/status`, `/model`, `/effort`, and `/compact`. The catalog
is derived from controller readiness plus native `model/list`; model names and
model-specific effort values are never copied into browser code.

Codex remains authoritative for effective thread settings, token accounting,
automatic context management, and compaction. Crew consumes thread start/resume
readback and `thread/settings/updated`, `thread/tokenUsage/updated`, and
compaction notifications. It may cache those projections for status rendering,
but it does not persist a competing model/effort configuration or implement a
second compaction threshold. Explicit model/effort changes use
`thread/settings/update`, then reconcile from native readback. Explicit
`/compact` is only a passthrough to `thread/compact/start`.

Each command emits Rust-sequenced `command_started`, `command_completed`, or
`command_failed` events keyed by its durable control ID. Native settings, usage,
and compaction notifications remain separate event facts on the same replay
stream.

### Interactions

Native command/file approvals, structured input, MCP elicitation, and permission
requests normalize to a generic pending-interaction record with:

- runtime, binding, thread, turn, and native request IDs;
- interaction kind and browser-safe prompt/options;
- requested, expiry, resolved, and stale timestamps;
- allowed response capabilities;
- resolution idempotency key and redacted outcome;
- optional bounded raw-detail reference.

Unknown server requests are persisted as unsupported attention events and
answered with a fail-closed JSON-RPC error. They are never auto-approved.

The initial local deployment may use `approval=never` and danger-full-access,
but the controller still implements all callback branches so a configuration
change cannot strand a turn silently.

## Controller Connection

One operational WebSocket connection multiplexes many threads for a registered
runtime. Requests and notifications route by exact JSON-RPC request ID,
`threadId`, and `turnId`; arrival order is not ownership.

The controller sequence is:

1. acquire the Rust-owned controller lease;
2. open Unix WebSocket with compression disabled;
3. initialize and verify version/capability fingerprints;
4. reconcile persisted bindings and active correlations;
5. begin accepting Rust-issued commands and native callbacks;
6. renew the lease while connected;
7. stop answering immediately on lease loss or protocol incompatibility.

The externally supervised attached process is not killed when Crew shuts down or
loses its lease. A future managed process mode must be a separate explicit
registration and may not be used as a hidden fallback.

## Capacity, Waiting, And Resume

Capacity belongs to active Crew work, not to the lifetime of a Codex thread.

- Acquire capacity when an admitted external turn begins.
- Release it exactly once when the turn becomes terminal.
- A pending native interaction is still an active turn and retains its turn
  capacity, while exposing attention state.
- A long external wait such as GitHub checks ends the native turn, records a
  Crew waiting state, and releases capacity.
- A terminal wait event schedules a new turn on the same native thread with
  structured provenance; it does not keep an old tool callback alive.
- Restart hydration may schedule reconciliation, but never blindly replay an
  uncertain non-idempotent turn.

This aligns den-services `#5500/#5501` with the external runtime lane without
making Review, Den, or TypeScript the scheduler.

## Direct Agent Messaging

Crew internal messaging remains runtime-neutral and Rust-owned.

- The durable target is a Crew agent/session identity, not a Codex thread ID.
- Dynamic coordination tools derive sender identity from the durable binding and
  controller lease. Model-supplied sender IDs are ignored/rejected.
- Message dedupe, TTL, expiry, queue order, pending round, reply correlation,
  timeout, late-reply disposition, and restart hydration use Crew persistence.
- Idle external targets start a new turn. Active targets queue for the next turn
  by default.
- A send-and-wait round suspends the caller through Crew coordination and resumes
  with a new turn/fact; it does not block a native tool request indefinitely.
- Codex-native subagents are observable native items. They are not Crew agent
  identities unless explicitly registered through the normal Crew path.

## Observation And Browser API

Codex remains native transcript authority. Crew persists a normalized,
browser-safe projection for replay and fleet attention:

```text
event_id
session_id
sequence_id
created_at
kind
source { runtime_kind, runtime_id, native_thread_id, native_turn_id? }
correlation { run_id?, project_id?, task_id?, item_id?, request_id? }
payload
raw_detail_ref?
```

Event families cover runtime/thread/turn lifecycle; message and reasoning
streams; tools, MCP, commands, and file changes; usage and compaction;
interactions; subagents; warnings; and unknown native notifications.

High-volume deltas use the chat/event replay path, not CoreBus. CoreBus receives
low-rate coordination facts such as turn terminal, interaction attention,
capacity release, and routed agent messages. Raw native details are bounded,
redacted, lazy, and operator-only.

Cursor domains are deliberately separate. `AgentMessageDeliveryReceipt.sequence`
is a CoreBus coordination sequence and must never be supplied as the `after` or
`Last-Event-ID` cursor for an external-runtime event feed. External event cursors
come only from `NormalizedExternalRuntimeEvent.sequenceId`. A caller that needs
durable lifecycle state follows `AgentActivation::ExternalTurnRequested.requestId`
through `GET /v1/external-turns/{request_id}`; it does not infer completion from
the delivery receipt remaining `accepted`.

Browser APIs advertise capabilities from the current runtime registration and
observed state. The UI does not infer that all external runtimes support every
control.

### Dispatch lifecycle field proof

Task `#5710` re-examined a delivery initially reported as never dispatched. The
durable SQLite records showed that request had actually started and completed in
about 3.4 seconds. Its delivery receipt used CoreBus sequence `48970`, while its
external runtime events used sequences `4326` through `4340`; polling the latter
with `after=48970` had hidden the successful turn.

After the lifecycle repair, the debug service certified both terminal paths:

- request `agent-message:task-5710-live-1783897289-message` completed native
  turn `019f5890-8224-7333-8450-ffcdbcaea381`, streamed
  `EXTERNAL_DISPATCH_5710_OK`, and read back as `completed` through the durable
  external-turn route;
- request `agent-message:task-5710-expiry-1783897330-message` used a one
  millisecond input TTL and read back as `failed` with
  `external_turn_dispatch_expired` without creating a native turn.

Both disposable native threads and bindings were cleaned through Crew's typed
hard-delete route after certification.

## Persistence

SQLite and PostgreSQL receive equivalent typed repositories for:

- external runtime registrations;
- controller leases and generations;
- external agent bindings;
- external turn requests and correlations;
- idempotent control receipts;
- pending interaction metadata and resolutions;
- normalized external events and cursors;
- bounded raw debug-detail metadata;
- direct-message queues/round correlations where not already covered by the
  coordination schema.

Foreign keys bind Crew-owned records to Crew sessions and agents. Native IDs are
opaque indexed strings with uniqueness scoped to their runtime/server identity.
No table mirrors Codex rollout files or full transcripts.

Retention rules:

- registrations, bindings, and non-secret fingerprints persist until archived;
- terminal turn/control facts follow session audit retention;
- high-volume normalized deltas may be compacted after durable message
  projection;
- interaction payloads and raw details use short configurable TTLs;
- secrets, auth headers, environment values, raw command environment, and full
  MCP payloads are never stored in external-runtime records;
- deletion of a Crew mapping does not delete a native Codex thread unless a
  future explicit destructive capability is designed.

Repository operations must use transactions and compare-and-swap revisions so
the same lease/idempotency behavior holds on SQLite and PostgreSQL.

## Restart And Failure Semantics

### Crew restart, app-server survives

The new Crew instance acquires a later lease generation, reconnects, verifies
the exact runtime, resumes known threads, reads native status, and reconciles
correlations. It does not create replacement threads for existing bindings.

### App-server completed restart

Crew reconnects and resumes exact persisted thread IDs. Dynamic Crew tools are
supplied again by the controller connection. Completed history remains native
truth.

### App-server dies during a turn

Crew marks the observed runtime disconnected and reconciles after replacement.
If native readback reports the turn interrupted/failed, Crew records that exact
terminal. If status is unknowable, Crew records `outcome_unknown`, releases
capacity once, and requires explicit retry/new-turn control. It never silently
replays the original turn.

### Pending server request is lost

The interaction becomes stale/lost. A late UI response is rejected. Reconnect
does not fabricate a response to an unknown native request ID.

### Protocol mismatch

A failed required method, malformed consumed shape, or missing initialize
capability places the runtime in `incompatible`. A different CLI version alone
does not. No thread/turn mutation is sent. Read-only diagnostics remain
available.

### Partial projection failure

Native execution may continue when chat/Den observation projection degrades.
Crew records projection lag and replays from persisted native/normalized cursors
where possible. Observation failure does not steal coordination authority or
cause duplicate native turns.

## Compatibility Boundary

Crew records the connected CLI version and the revision of the protocol subset
its controller consumes. A CLI version change is not itself an incompatibility.
After initialization, the controller reports whether the required consumed
contract is compatible; Rust records the result as `compatible_uncertified` or
`incompatible` and remains authoritative for turn admission.

The controller runs the bounded `codex-required-capabilities-v1` probe before
asking Rust for admission. It compiles the consumed codecs, validates the Crew
dynamic-tool registration, and probes `model/list`, `thread/list`,
`thread/read`, and `thread/resume` without creating a test thread. Each step,
duration, stable failure reason, observed CLI version, and consumed-contract
revision are persisted with the runtime registration and exposed by controller
diagnostics. Transport failures remain `unassessed` and retryable; semantic
failures are `incompatible` and do not enter a reconnect loop.

Additive fields and unknown methods must not require a Crew release when the
consumed operations remain valid. Missing required operations, malformed known
messages, or failed required capability probes are concrete incompatibilities
and must block turns with stable reason codes.

Durable `certified` status and debug-to-live promotion are owned by the Codex
compatibility workflow rooted at task `#5783`. A successful handshake remains
`compatible_uncertified` until the exact CLI version, consumed-contract
revision, and probe-suite revision have a passing debug certification. The
guarded live promotion records a separate live certification after it preserves
binding/thread identity and proves a fresh controller lease.

Routine Codex updates do not edit source version pins or regenerate protocol
artifacts merely because the CLI version changed. Operators stage the installed
CLI on debug, run `npm run codex:debug:update-certify`, and then run
`npm run codex:live:promote`. Regeneration is required only when Crew deliberately
changes the generated protocol baseline or its consumed contract. Generated
schemas still drive bridge types; hand-maintained TS mirrors may not become
compatibility-policy authority.

## Existing Direct Brains

Existing Chat Completions and OpenAI Responses modules remain direct brains and
remain
the default for profiles that select them. No existing profile silently switches
to Codex. External runtime binding is explicit and exclusive with direct-brain
execution for a given session.

The direct Responses brain continues to own Rusty replay/chaining semantics. The
Codex lane uses Codex's own Responses/provider behavior. Provider state does not
cross between those lanes.

## Rejected Alternatives

### Model Codex as a BrainModule

Rejected because native turns do not terminate in `BrainAction`, and native
tools/commands/subagents must remain Codex-owned.

### Put lifecycle authority in the TS driver

Rejected because reconnects, capacity, message routing, waits, and idempotency
must survive driver replacement and remain consistent across storage backends.

### Let Rusty View connect directly

Rejected because it creates competing controllers, exposes native protocol and
local endpoint details to a browser, and makes browser lifetime part of agent
correctness.

### Ship managed stdio first

Rejected as the production architecture after `#5517`. It is simpler framing
but loses independent supervision and Crew-restart adoption already proven by
the Unix service. It remains a diagnostic oracle only.

### Generic fallback among transports

Rejected. Hidden fallback changes process ownership and recovery semantics. A
registration names one transport/ownership mode and fails visibly if unavailable.

## Consequences

- The architecture gains a second execution lane, but not a second coordination
  authority.
- Rust contracts and persistence work are larger before the first UI slice, in
  exchange for restart-safe behavior and fewer temporary paths.
- TypeScript receives a narrow, versioned transport/codec role.
- Rusty View can remain a generic Crew client and gain Codex capability through
  normalized APIs.
- Codex improvements remain available because Crew does not reimplement its
  loop.
- Runtime upgrades become explicit operational events with compatibility gates.
- Live testing must cover both the external process boundary and the browser
  projection; deterministic protocol fixtures alone are not deliverable proof.

## Implementation Order

1. Add Rust protocol/persistence/runtime registration, lease, binding, turn,
   control, interaction, and normalized-event contracts.
2. Generate the TS protocol baseline, define the consumed-contract boundary,
   and implement the attached Unix driver under Rust commands.
3. Add runtime registration/admin diagnostics and browser-safe capability APIs.
4. Implement thread discovery/attachment and read-only projection.
5. Implement turn start/stream/control and interaction broker.
6. Integrate Crew dynamic coordination tools and runtime-neutral direct
   messaging.
7. Add restart, stale request, capacity, wait/resume, and cross-backend tests.
8. Certify a real coding turn through the deployed debug service and Rusty View.
9. Stage future CLI updates through debug certification and guarded live
   promotion; revise source contracts only when the consumed protocol changes.
