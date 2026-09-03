# Rusty Crew Successor-Service Surface Survey

**Status:** architecture survey; no implementation decision  
**Date:** 2026-08-21  
**Scope:** current /home/dev/rusty-crew plus the candidate messaging fabric in /home/dev/crew-services  
**Authority:** current code and the Den rusty-crew-unified-architecture document. Where older local planning notes disagree, the unified architecture and code win.

## Result

Rusty Crew should not be split by moving its coordination core into a group of remote services. The Rust coordination authority intentionally shares one state machine for session identity, internal routing, body projection, wake eligibility, action validation, delegation, logical turns, and restart hydration.

The useful successor pattern is narrower:

1. Retain one Rust-owned coordination authority.
2. Promote only external concerns with a self-contained contract, state, and failure model into independently operated services or workers.
3. Finish the existing internal decomposition of service-app, persistence, and control-plane seams before treating them as network boundaries.

Crew Services validates this approach. It is an optional, runtime-neutral delivery fabric behind a Rusty Crew adapter; it is not a Rusty Crew extraction or replacement for the internal bus.

## Current authority map

~~~
external channel / fabric / MCP / provider / operator client
             |
             | normalized input, adapter state, typed ports
             v
service-host (process composition, timers, listener, adapter factories)
             |
             v
brain-island (tools, profile/role assembly, HTTP envelopes, provider glue)
             |
             | native bridge / manifest operations
             v
Rust coordination authority
  core-engine + session + body + bus + persistence + config
             |
             v
durable Crew service data and accepted coordination effects
~~~

An adapter/service boundary is not an authority boundary. An adapter may own a connection, remote cursor, retry/backoff, or delivery-fabric claim. It must hand a typed request to Rust for final route/session validation and non-interrupting wake behavior. It does not own the resulting session state or decide whether a running turn is steered.

## Classification

| Surface | Present coupling | Successor judgement |
| --- | --- | --- |
| core-engine bootstrap and composition | Rehydrates sessions/events, fences prior service instances, reconciles delegated/external/logical work, and composes bus, body, registry, and persistence. | **Keep one Rust authority.** It is the eventual service boundary as a whole, not a source of independently deployable methods. |
| core-bus, core-session, core-body, brain action execution | In-process history/subscriptions, session revision/lifecycle, frozen snapshots, wake threshold, and action effects share ordering. | **Extremely coupled.** A future remote form needs durable event/outbox/subscription semantics, not a wrapper over mpsc. |
| Agent routing, delivery, replies, rounds | Route resolution, identity/revision checks, durable receipts/queues, TTL, idempotency, and wake emission are Rust coordination facts. | **Keep Rust authoritative.** A fabric may transport/ledger messages, but cannot be a second routing authority. |
| Delegation and logical turns | Lineage, capacity/lease claims, continuation tickets, completion wakes, cancellation, and restart reconciliation cross registry, store, bus, and engine. | **Future bounded domains, not early extractions.** Complete state-machine and fencing semantics first. |
| Runtime graph/config planning | Profiles, bindings, sessions, storage mode, and scheduled work affect coordination. Target ownership is Rust core-config planning, with TS applying an accepted plan. | **Finish the Rust control-plane seam first.** Do not move hidden policy into a new TS/service authority. |
| Native bridge and protocol | Explicit manifest-owned operations and transport-free types already form a border. | **Strong contract seam.** Keep it narrow; split coordination DTO roots from feature/admin DTOs before publishing a broad service API. |
| service-host | Listener/static shell, startup/preflight, factories, timers, and stop ordering. | **Process-composition candidate.** Continue moving timers and adapter lifecycle here; it may remain in one Crew deployment. |
| brain-island / service-app.ts | The current 7.8k-line transitional app combines route handling, runtime rebuild, adapter startup, wake/drain state, diagnostics, and callbacks. | **Internal decomposition first.** Route modules and explicit ports are useful; this is not currently an independently authoritative service. |
| Platform adapters | Den, Telegram, MCP, TUI, and Crew Services have explicit packages/ports and external failure modes. | **Best service/worker candidates.** Keep them as transport/normalization/egress boundaries with typed core ingress. |
| Crew storage repositories | Repositories partition sessions, events, queues, scheduler/workers, profiles/providers, conversations, memory/lore, telemetry, and admin, but share backend/migrations. | **Repository decomposition, not automatic services.** Promote only concerns with an independent lifecycle, conformance suite, and logical transfer story. |
| Chat/transcript projections | Rust owns content mutation/order/branches; TS owns HTTP/SSE envelopes. | **Read-model/API candidate later.** Preserve mutation authority and ordered replay before splitting a browser-facing projection. |
| Diagnostics/TUI/query catalog | Read-only public diagnostics over typed native APIs. | **Safest independent consumer.** It must remain storage-private and non-mutating. |

## What the Crew Services candidate demonstrates

Crew Services is a local Go/SQLite, loopback delivery fabric. Its deliberate scope is runtime-neutral: it does not know Rusty Crew sessions, brains, providers, transcripts, or native activation. Its implemented foundation has:

- immutable message acceptance separated from mutable delivery state;
- producer-scoped idempotency with request-fingerprint conflict detection;
- revisioned address bindings, adapter leases, and claim fencing;
- FIFO claims and an explicit queued -> claimed -> dispatching -> delivered ledger;
- pre-native-dispatch release, post-native-dispatch reconciliation, and outcome_unknown rather than unsafe blind redelivery;
- TTL/reaping plus exact linked replies and one-time round resolution.

Rusty Crew's adapter-crew-services package already expresses the correct relationship. It synchronizes exact aliases/route revisions, calls native deliver_agent_message, and reconciles the fabric delivery against the Rust receipt. Its real-boundary smoke creates disposable direct-brain routes and proves ordinary, replay, and linked-reply translation.

Crew Services is therefore a useful **behavioral donor and optional external fabric**, particularly for immutable acceptance versus side-effect ledgers, generation fencing, adapter crash/restart reconciliation, non-interrupting busy-to-next-turn delivery, and inspection/replay.

It should not become the in-process bus or duplicate Rust's direct delivery, session activation, or wake-policy authority. Running both as canonical owner of routes, queues, replies, or TTL would create split-brain coordination.

The currently assembled adapter remains intentionally limited to local, direct-brain bindings. Managed-Codex delivery, busy-to-idle UI timing, and fault-injected begin/restart behavior are not yet certified by its real-boundary smoke.

## Good successor candidates

| Priority | Candidate | It may own | Rusty Crew retains |
| --- | --- | --- | --- |
| 1 | Den Channels / Successor Gateway worker | Transport selection, reconnect, cursors, source-shape normalization, remote HTTP/auth/retry, activity projection. | Binding validity, final route validation, expiry/idempotency, session activation, wake eligibility, durable Crew facts. |
| 1 | Telegram connector worker | Bot polling, offsets, media transfer, transport retry, remote terminal/quarantine handling. | Session lifecycle and final typed message acceptance. |
| 1 | MCP transport/discovery manager | Remote connection state, discovery, reconnect/backoff, server diagnostics. | Tool metadata policy/registration, profile selection, session tool execution. |
| 2 | Crew Services fabric client/pump | Fabric leases, alias binding synchronization, claim/reconciliation loop, fabric diagnostics. | Exact route/session checks, delivery acceptance, body/wake policy, transcript and completion truth. |
| 2 | Service-host composition | Process timers, listener/static assets, adapter startup/drain, explicit port wiring. | Durable scheduling decisions and coordination effects. |
| 2 | Read-only operator clients | Diagnostics, event/session/tool/channel inspection through public APIs. | Storage access and mutation authority. |
| 3 | Brain-host deployment boundary | A transport-stable provider/tool execution host. | Brain registration, wake policy, continuation identity, session/provider-state authority, accepted action effects. |

The Den, Telegram, and MCP lanes come first because they already depend on outside transport state rather than Crew-only lifecycle data. They should be treated as trusted-local adapter workers, not retrofitted with unrequested public-edge or multi-tenant machinery.

## Important non-candidates for an early split

### Coordination kernel

The kernel's in-process implementation is intentionally enforceable. CoreBus uses local ordered history/subscriptions; BodyProjector reads that history with SessionRegistry; CoreEngine handles delegation and lifecycle cases outside generic action execution. Replacing any part independently would fracture atomic ordering and restart semantics.

If a future product needs a coordination-service boundary, move the complete Rust authority behind one typed facade. Preserve event ordering, filtered replay, idempotent wake IDs, service-instance epochs, action validation, and durable outbox/receipt semantics. Do not publish private bus or registry implementation details as convenience RPCs.

### Storage as microservices

core-persistence has meaningful concern partitions and grouped facades. That is the correct current shape. Separate deployment is premature for most partitions because they still share backend bootstrap, migration ordering, transactions, logical export/import, and coordination invariants. Queues, scheduler claims, worker leases, logical turns, and event facts are one correctness cluster. Memory, lore, and roleplay should not become services merely to make a storage directory smaller.

The safest storage-facing pilot is a typed, read-only storage-admin/query boundary: readiness, capability/schema diagnostics, maintenance, curated row counts, and projections. It must never supply arbitrary SQL or direct database access to TypeScript or clients.

### service-app as an authority island

The oversized app is evidence for local decomposition, not evidence that its mixed responsibilities belong in a standalone TypeScript service. Existing route-family extraction and service-host ports are the preferred remediation. Each follow-up should be labelled as:

- **migrated:** production behavior now depends on Rust/codegen authority;
- **ratcheted:** a check prevents further sprawl;
- **certified-current-boundary:** TypeScript remains the intentional owner; or
- **planned:** only a named future lane.

## Required contracts before promotion

No candidate becomes a separately operated service based only on source modularity.

1. **One authoritative owner.** Name the owner of every identity, revision, transition, and terminal record. Never duplicate session, route, queue, or wake decisions.
2. **Typed boundary.** Use a versioned generated/checked DTO or narrow bridge operation. Do not pass private Rust/TypeScript state bags.
3. **Idempotency and fencing.** Include operation identity, payload fingerprint where needed, binding/service generation, leases/claims, and reconciliation for ambiguous effects.
4. **Failure semantics.** Define retry, expiry, terminal state, cancellation, shutdown, and adapter-degraded behavior without blocking internal coordination.
5. **Durability and recovery.** Prove restart, replay/ordering, no-resurrection after TTL/terminal state, and service-instance fencing.
6. **Storage portability.** When durable Crew data moves, use logical export/import with stable IDs, module/schema versions, dry-run/idempotency, counts/checksums, secret exclusion, and capability checks. Do not copy SQLite files.
7. **Focused proof.** Add a narrow deterministic contract test first. Use a real-boundary smoke only for a process/transport claim; keep provider and UI certification separate.

## Suggested sequence

### Phase A — make seams authoritative inside the current deployment

1. Finish the Rust runtime-graph plan/apply clean break.
2. Move process timers, adapter start/stop, and preflight into service-host, retaining narrow executor ports.
3. Finish Rust route-planning/validation for normalized channel input before operating an ingress worker independently.
4. Reduce the direct Crew Services adapter dependency in service-app to a narrow delivery port without changing direct-brain protocol semantics.
5. Continue storage repository/port conformance, especially lifecycle queues, scheduler claims, logical turns, and backend capabilities.

### Phase B — externalize low-authority adapters

1. Pilot a Den transport/normalizer worker or Telegram worker with normalized ingress plus rebind/expiry/idempotency proof.
2. Pilot MCP transport/discovery as a sidecar while Rust continues metadata validation and brain-island performs tool execution.
3. Expand the Crew Services pump only when its delivery-port contract makes replay, revision drift, and uncertain native acceptance observable and recoverable.

### Phase C — reconsider higher-authority services only for a concrete product need

Only after the above seams are proven should the project evaluate a dedicated brain host, logical-turn authority, delegation authority, or complete Rust coordination service. Each is a product/availability decision, not a cleanup exercise.

## Evidence and limitations

Primary implementation evidence includes:

- crates/core/core-engine/src/{bootstrap,agent_coordination,delegation,logical_turns}.rs;
- crates/core/core-{bus,session,body,persistence}/src/;
- crates/bridge/core-bridge-api/bridge-manifest.toml and crates/bridge/core-bridge-node/src/;
- ts/packages/{service-host,brain-island,adapter-den,adapter-telegram,adapter-mcp,adapter-crew-services}/src/; and
- /home/dev/crew-services/{README.md,AGENTS.md,internal/} as donor/candidate evidence only.

Relevant local architecture references include docs/service-composition-decomposition-plan.md, docs/service-app-decomposition-inventory-2026-07-09.md, docs/adr/0022-crew-owned-service-storage.md, docs/persistence-boundary-portability-contract.md, docs/platform-adapter-anticorruption-audit.md, and docs/core-engine-domain-module-map.md.

This was a read-only source and architecture survey. No services were deployed, no code was changed, and no live behavior was certified. Several older decomposition documents retain useful contracts but stale line counts or pre-cutover ownership language; this survey uses current source for current implementation claims.

