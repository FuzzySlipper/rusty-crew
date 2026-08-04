# Codex Dynamic Tool Catalog Reconciliation

## Decision

Rusty Crew persists the fingerprint of the exact Rusty Crew dynamic-tool
catalog applied when a Codex app-server thread is started. The fingerprint is
stored on `ExternalAgentBinding.dynamicToolCatalogFingerprint`, alongside the
native thread identity and profile provenance.

On reconnect or controller recovery:

1. Crew computes the catalog required by the binding's agent/profile identity.
2. An equal fingerprint resumes the exact native thread with no replacement.
3. A missing or different fingerprint is stale/unknown. When the thread is
   idle, Crew starts a replacement with the current catalog, preserves the
   Crew binding/session/agent/profile identity and thread settings, persists
   the new binding revision, and archives the old native thread.
4. The replacement is idempotent: its source contains the binding, catalog
   fingerprint, and replaced thread identity, so a restart during the repair
   can recover the already-created candidate instead of creating another one.
5. A running turn or pending native interaction is never replaced. Crew
   resumes the exact old thread and exposes the stale-catalog condition in
   runtime diagnostics so the next idle reconnect can retry.

Catalog replacement emits a durable
`dynamic_tool_catalog_refreshed` external-runtime event containing the old and
new native thread IDs and the applied fingerprint. The old binding revision,
Crew session, label, task reference, and routed identity remain the authority;
the native thread ID is the only external identity that changes.

## Why Replacement Is Required

The consumed Codex app-server contract accepts `dynamicTools` on
`thread/start`. It does not expose a supported dynamic-tool update on
`thread/resume`, `thread/settings/update`, `turn/start`, or `thread/fork`.
`thread/resume` therefore cannot add a tool to an already-persisted native
thread. A binding/profile refresh that only updates Crew metadata would report
success while the provider still lacks the tool.

The catalog fingerprint is deliberately separate from
`effectiveConfigFingerprint`. The latter describes the Crew runtime/profile
configuration and is Rust-owned; the dynamic catalog is assembled at the
Codex adapter boundary from the exact tool specs supplied to `thread/start`.

Reviewer bindings use the reviewer catalog fingerprint, so a reviewer refresh
proves the managed set includes `complete_routed_review` and excludes the raw
`reply_agent_message` primitive. Direct and recovery identities continue to
use the full catalog.

## Context Boundary

Codex does not currently provide a stable local app-server operation that
combines an existing persisted thread history with a changed dynamic-tool
catalog. `thread/resume.history` is an unstable Codex Cloud contract and
`thread/fork` does not accept a new dynamic catalog. Crew therefore retains the
old native thread as an archived artifact and retains the durable Crew
projection, but does not pretend that a replacement has native conversation
history identical to the old thread.

This is an explicit, observable replacement rather than a hidden fallback or a
silent session reset. A future history-transfer implementation must add a
versioned handoff/reconstruction contract and proof that user/tool context is
preserved before it is enabled. Ordinary service restarts with an unchanged
catalog remain exact-thread resumes.

## Rejected Alternatives

- **Always start a new thread on restart:** destroys long-lived Codex context
  even when no tool change occurred.
- **Update only the Crew profile/binding metadata:** leaves the native Codex
  catalog stale and makes the API claim more capability than the provider has.
- **Send a tool description as a user message:** does not register a callable
  dynamic tool and pollutes model context.
- **Call Codex's private state database directly:** couples Crew to an internal
  storage schema and cannot provide a stable cross-version contract.

## Verification Requirements

The deterministic controller coverage must prove all of the following:

- current catalog resumes the same native thread;
- unknown/stale catalog replaces once and archives the previous thread;
- a second reconnect resumes the replacement without another `thread/start`;
- reviewer catalog identity is preserved through replacement;
- replacement preserves Crew identity, settings, label, and task reference;
- replacement events expose old/new thread IDs and the fingerprint;
- active turns and pending interactions defer replacement and surface the
  stale state;
- a restart during replacement recovers the idempotent candidate.

Live certification must create a long-lived managed reviewer before a tool is
added or its catalog revision changes, reconnect the Codex app server, route a
fresh review wake, and verify one successful `complete_routed_review` call and
one correlated reply without routing metadata being exposed to the model.
