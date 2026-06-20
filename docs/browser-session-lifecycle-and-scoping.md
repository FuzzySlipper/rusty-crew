# Browser Session Lifecycle And Scoping

Status: Design contract for task 2885

Date: 2026-06-20

## Decision

Rusty Crew browser state is scoped per Rust `SessionId`.

It is not scoped per wake, because multi-step browser work needs continuity. It
is not scoped per profile, because one hosted service may run many agents with
the same profile and must avoid cross-agent leakage. It is not a process-global
singleton, because hidden globals made the pi-crew browser surface hard to
reason about.

The TypeScript brain island owns Chromium/CDP process control and volatile page
state. Rust owns the durable session identity, selected tool profile, lifecycle
signals, cancellation/deadline policy, resource limits, and telemetry hooks.

## Browser Manager Contract

The implementation should introduce a `BrowserSessionManager` in the brain
island. Tools receive or resolve this manager through their normal resolver
context instead of importing a module-global session map.

Minimum API shape:

```ts
interface BrowserSessionManager {
  open(input: BrowserOpenInput, signal?: AbortSignal): Promise<BrowserSessionHandle>;
  snapshot(sessionId: SessionId): BrowserSnapshot | undefined;
  close(sessionId: SessionId, reason: BrowserCloseReason): Promise<void>;
  closeAllForAgent(agentId: AgentId, reason: BrowserCloseReason): Promise<void>;
  sweep(now?: Date): Promise<BrowserCleanupSummary>;
  diagnostics(): BrowserManagerDiagnostics;
}
```

The manager may be an in-memory TS object in v1, but it must be injected so
tests and later service wiring can own its lifecycle deliberately.

## Session Record

Each browser session record is keyed by Rust `SessionId` and stores:

- `sessionId`, `agentId`, `profileId`, and optional parent/delegation lineage
  identifiers for diagnostics.
- Browser process handle and CDP connection handle.
- Ephemeral user-data directory path.
- Current top-level URL and title, when known.
- Creation time, last-used time, and last navigation time.
- Snapshot generation number.
- Ref cache for the latest snapshot generation.
- Bounded console ring buffer.
- Last error and close reason.
- Current state: `starting`, `ready`, `closing`, `closed`, or `crashed`.

Do not persist this record as durable runtime state. Persist only telemetry and
high-level lifecycle events.

## Ref Model

`browser_snapshot` creates refs; interaction tools consume refs.

Rules:

- Refs are scoped to one Rust `SessionId`.
- Refs include a snapshot generation internally, for example
  `browser:<sessionId>:<generation>:<refId>`.
- Model-facing refs may remain compact, such as `@e0`, but the manager must map
  compact refs through the session/generation of the latest snapshot.
- Any navigation, reload, back, or page crash increments the generation and
  invalidates old refs.
- Stale refs fail closed with `stale_browser_ref` and a hint to call
  `browser_snapshot` again.
- Refs should point to stable internal selectors or backend node ids where
  available. If CSS selectors are used in v1, they remain TS-local and must not
  be exposed as durable data.
- Snapshot output is bounded by element count and text length.

The pi-crew ref idea is worth adapting; its module-global `sessions` map and
raw selector cache should not be copied as-is.

## Process Lifecycle

Browser sessions are lazy-started on the first browser tool call for a selected
session.

Startup:

- Resolve the browser binary from runtime config, not model input.
- Use an ephemeral user-data directory per Rust `SessionId`.
- Start Chromium headless with remote debugging bound to loopback.
- Open exactly one default page/context for v1.
- Record startup telemetry with session/profile/agent ids and configured limits.

Runtime:

- Update `lastUsedAt` on every browser tool call.
- Apply per-call timeout/deadline from Rust-owned resource policy.
- Pass `AbortSignal` into CDP/fetch/wait operations where available.
- Bound concurrent operations per browser session to one mutating action at a
  time. Read-only snapshot/console calls may queue behind mutation rather than
  racing page state.

Shutdown:

- Close CDP first, then terminate Chromium.
- Remove the ephemeral user-data directory.
- Mark all refs stale.
- Emit lifecycle telemetry.

Restart:

- Browser sessions are not restored after process restart.
- Any persisted or model-held refs become invalid.
- Diagnostics should report a dropped/stale session if Rust asks about a session
  whose browser process is gone.

## Cleanup Policy

Required limits:

- Max browser sessions per hosted service.
- Max browser sessions per agent.
- Optional max browser sessions per profile.
- Idle timeout per browser session.
- Hard lifetime cap per browser session.
- Startup timeout.
- Per-CDP-call timeout.
- Console ring size.
- Snapshot/ref cache size.
- Screenshot byte/artifact cap.

Default recommendation for initial implementation:

- service max: 8 sessions;
- per-agent max: 2 sessions;
- per-profile max: unset unless operators configure it;
- idle timeout: 10 minutes;
- hard lifetime: 60 minutes;
- startup timeout: 15 seconds;
- CDP call timeout: 15 seconds;
- snapshot max interactive refs: 80;
- console ring size: 100 entries.

If a limit would be exceeded, the manager should deny the new browser session
with a resource-denial reason instead of evicting another active session
silently.

## Lifecycle Signals From Rust

Rust should eventually send or expose these lifecycle inputs to TS:

- `session_archived`: close the matching browser session.
- `agent_archived` or equivalent future control event: close all sessions for an
  agent.
- service drain/shutdown: close all sessions with bounded timeout.
- wake cancellation: abort the active browser operation for that wake.
- selected `ToolProfile` or resource limits: decide whether browser tools are
  allowed and what limits apply.

V1 can expose explicit manager methods and smoke tests before the full Rust
event bridge exists. The implementation should keep the API ready for those
signals.

## Diagnostics

Diagnostics should expose summaries only:

- active browser session count;
- sessions by agent/profile;
- state, current URL/title, created age, idle age, and lifetime age;
- snapshot generation and ref count;
- console ring size;
- process id when safe for local operator diagnostics;
- last error/close reason;
- configured limits;
- cleanup counts.

Diagnostics must not expose cookies, local storage, full DOM snapshots, raw
screenshots, auth headers, or hidden form values.

## Tool Behavior Implications

- `browser_navigate` opens the session if needed, validates top-level URL policy,
  navigates, waits for bounded load, increments snapshot generation, and returns
  URL/title/status details.
- `browser_snapshot` opens the session if needed and returns a bounded
  accessibility/interactive-element view.
- `browser_click`, `browser_type`, and `browser_press` require a current ref or
  focused page state and run as mutating operations.
- `browser_scroll` and `browser_back` mutate page state enough to invalidate
  refs.
- `browser_console` reads the bounded console ring; it does not evaluate
  arbitrary JavaScript in v1.
- `browser_vision` captures a screenshot artifact reference, not inline base64
  and not a direct model call.

## Concurrency Rules

Within one browser session:

- Mutating browser tools are sequential.
- Snapshot and console calls may run only when no mutation is in progress.
- A cancellation aborts the active operation and leaves the session either
  `ready` or `crashed`; uncertain state becomes `crashed` and requires restart.

Across sessions:

- Sessions may run concurrently up to service/agent/profile limits.
- Delegated child sessions do not inherit parent browser state by default.
- Future explicit browser sharing requires a visible lease/policy object.

## Implementation Notes

Safe to reuse from pi-crew:

- CDP client request/response structure.
- `DevToolsActivePort` discovery idea.
- Accessibility snapshot and compact ref presentation.
- Console ring-buffer concept.

Must change for Rusty Crew:

- Replace module-global `sessions` with an injected manager.
- Key all state by Rust `SessionId`, not profile or implicit singleton.
- Add explicit generation-based ref invalidation.
- Add cleanup, sweep, close, and diagnostics APIs before broad tool rollout.
- Replace inline screenshot/base64 return with artifact refs.
- Keep browser binary and private-network policy in runtime config, not tool
  input.
