# Provider Wire-State Persistence

Status: design contract for review

Date: 2026-06-23

Related task: #3229

Related docs:

- `[doc: rusty-crew/modular-brain-boundary-design-2026-06-23]`
- `[doc: rusty-crew/brain-island-rust-ownership-audit-2026-06-23]`

## Purpose

Some non-pi brain modules need provider-specific state across wakes. Examples
include OpenAI Responses replay metadata, `previous_response_id` chaining, or
provider cache handles.

Rusty Crew should persist this state because it crosses wake and restart
boundaries, but Rust must not interpret it as coordination state. Provider wire
state is an opaque brain-module artifact. It cannot route messages, validate
actions, wake sessions, authorize tools, or replace Rust-owned session/body
history.

## Ownership

Rust owns:

- record namespace, expiry, invalidation, and persistence;
- passing the current opaque payload into a brain wake;
- accepting an updated opaque payload after a brain wake;
- deleting or superseding records when the selected module/strategy/fingerprint
  changes.

The brain module owns:

- payload schema and versioning;
- provider semantics such as response ids, replay windows, cache markers, and
  provider item references;
- deciding whether missing state is recoverable for its strategy;
- emitting a new state payload or explicitly clearing state.

Product data remains in Den. Coordination state remains in Rust sessions,
events, queues, and body state. Provider wire state is neither.

## Record Shape

Persist one current record per `(session_id, module_id, strategy_id)` plus
historical metadata useful for audit/debug.

```ts
export interface ProviderWireStateRecord {
  readonly sessionId: string;
  readonly moduleId: string;
  readonly strategyId: string;
  readonly profileFingerprint: string;
  readonly providerFingerprint: string;
  readonly payloadVersion: string;
  readonly payloadJson: unknown;
  readonly payloadEncoding: "json";
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly expiresAt?: string;
  readonly lastWakeId?: string;
  readonly invalidatedAt?: string;
  readonly invalidationReason?:
    | "profile_changed"
    | "provider_changed"
    | "module_changed"
    | "strategy_changed"
    | "expired"
    | "brain_requested_clear"
    | "operator_requested_clear";
}
```

The first persistence implementation can store `payloadJson` as opaque JSON
text. If a future module needs encrypted binary payloads, add a new
`payloadEncoding` value and keep Rust limited to storage and size/expiry policy.

### Identity Fields

`sessionId` is the Rust session receiving wakes.

`moduleId` is the selected `BrainModule.moduleId`, for example
`pi-agent-core`, `openai-responses`, or `local`.

`strategyId` is the module's internal state strategy, for example `replay` or
`previous-response-chain`. A module with no cross-wake state should omit state
entirely rather than writing a sentinel record.

### Fingerprints

`profileFingerprint` should cover fields that change prompt or agent identity
semantics, including profile id, selected brain module/strategy, soul/memory
prompt material, tool profile identity, and any profile options the module says
affect provider continuity.

`providerFingerprint` should cover fields that change provider continuity,
including provider kind, model name, base URL, API mode, API key identity
reference, temperature/effort/reasoning settings when relevant, and module
strategy settings.

Fingerprints are compared by Rust for invalidation. Rust does not need to know
why a field matters beyond receiving the canonical fingerprint values from the
profile/module selection layer.

## Wake API Shape

Provider state should be carried beside the existing frozen wake input and wake
result. It should not be embedded in body state.

```ts
export interface BrainWakeProviderStateInput {
  readonly moduleId: string;
  readonly strategyId: string;
  readonly profileFingerprint: string;
  readonly providerFingerprint: string;
  readonly payloadVersion: string;
  readonly payload: unknown;
  readonly expiresAt?: string;
}

export interface BrainWakeProviderStateUpdate {
  readonly moduleId: string;
  readonly strategyId: string;
  readonly profileFingerprint: string;
  readonly providerFingerprint: string;
  readonly payloadVersion: string;
  readonly payload: unknown;
  readonly ttlMs?: number;
}

export type BrainWakeProviderStateOutput =
  | { readonly type: "unchanged" }
  | { readonly type: "replace"; readonly state: BrainWakeProviderStateUpdate }
  | { readonly type: "clear"; readonly reason: "brain_requested_clear" };
```

The bridge-facing wake input should expose at most one provider state record for
the selected module/strategy. If no valid record exists, pass no provider state
and set an explicit absence reason in diagnostics:

```ts
type ProviderStateAbsenceReason =
  | "not_configured"
  | "missing"
  | "expired"
  | "invalidated"
  | "module_does_not_use_state";
```

The wake result should contain provider state output separately from
`BrainAction[]`. Provider state updates are persistence hints, not actions. They
must not be accepted through the action validator or exposed as model-callable
tool side effects.

## Invalidation Rules

Before every wake, Rust should compare the current selected
`(moduleId, strategyId, profileFingerprint, providerFingerprint)` against the
stored record.

Invalidate and withhold the record when:

- the session id differs;
- the selected module id differs;
- the selected strategy id differs;
- the profile fingerprint differs;
- the provider fingerprint differs;
- `expiresAt` is present and is not in the future;
- the operator or module explicitly cleared state.

Invalidation should mark the existing record non-current before the wake. The
next wake proceeds without provider state unless the module declares that the
state is required.

Profile and config refresh should therefore not need to restart the service to
be safe. Rebuilding a brain/session after profile changes should naturally
produce new fingerprints and invalidate stale provider state.

## Expiry And Missing State

Provider wire state must be TTL-bound by default. Recommended initial defaults:

- replay/cache metadata: 24 hours unless module config overrides lower;
- `previous_response_id` chaining: 6 hours unless module config overrides lower;
- explicit maximum TTL cap: 7 days for any provider state record.

On missing or expired state:

- replay-capable modules should reconstruct from Rust-owned message/body history
  and write fresh state after the wake;
- chaining-only modules may start a new provider chain when safe;
- modules that cannot recover must fail the wake with a brain-unavailable style
  error that is visible in diagnostics, rather than silently reusing stale
  state.

Expired state must never be replayed to the provider. Expired records may remain
available to debug/admin inspection if payload visibility rules allow it, but
they are not deliverable runtime state.

## Strategy Comparison

### Responses Replay / Prefix-Cache Strategy

The module stores replay assist metadata, such as provider item ids, cache
markers, truncation watermarks, or summaries that help it rebuild a Responses
request from Rust-owned history.

Pros:

- missing state is usually recoverable from Rust-owned message/body history;
- profile/model changes can safely invalidate state and rebuild;
- avoids depending on an unbroken provider-side previous-response chain;
- fits Rusty Crew's frozen wake model because provider continuity is derived at
  wake boundaries.

Costs:

- larger requests when cache hints miss or expire;
- module must carefully map Rust history into provider input items;
- token/cost behavior depends on provider cache policy and is harder to
  guarantee.

This should be the conservative first Responses strategy.

### Responses `previous_response_id` Chaining Strategy

The module stores the last provider response id and sends it as the predecessor
for the next provider call.

Pros:

- compact follow-up requests;
- can preserve provider-side reasoning/context continuity when supported;
- may reduce token use for stable sessions.

Costs:

- missing or expired state can sever the chain;
- provider-side retention and deletion become operational concerns;
- profile/model/tool changes are more hazardous and must invalidate the chain;
- rollback/rewind semantics are harder because Rust does not own provider-side
  history.

This strategy should be added only behind an explicit strategy id and diagnostic
surface. It must have a clear fallback behavior when the stored predecessor is
missing, expired, rejected, or no longer compatible.

## Failure Behavior

Provider state persistence failure must not corrupt coordination state.

Before wake:

- if loading state fails, wake without provider state only when the selected
  strategy declares state optional;
- if the strategy declares state required, fail the wake before calling the
  provider and surface the failure in diagnostics.

After wake:

- if saving updated state fails after model output/actions are produced, Rust
  may still process valid brain events/actions;
- the save failure should be recorded as degraded provider-state persistence so
  the next wake can recover or fail according to strategy rules;
- never retry a completed model call just to recover provider state.

## Diagnostics

Runtime diagnostics should show, without dumping opaque payload by default:

- selected module id and strategy id;
- provider state status: `unused`, `valid`, `missing`, `expired`,
  `invalidated`, `load_failed`, or `save_failed`;
- timestamps and expiry;
- payload size in bytes;
- last wake id;
- invalidation reason when present.

Raw payload inspection, if ever added, belongs behind an explicit debug route or
operator tool with redaction/size limits.

## Implementation Phasing

After this design is reviewed:

1. Add protocol/bridge types for provider state input/output beside
   `BrainWakeInput` and `BrainWakeResult`.
2. Add Rust persistence for current provider state records with TTL cleanup and
   invalidation.
3. Add module-selection fingerprints in the TypeScript service/profile layer.
4. Add diagnostics and smokes for missing, expired, invalidated, and replaced
   state.
5. Implement the Responses replay strategy first.
6. Consider `previous_response_id` chaining after replay is proven in service
   field tests.

## Non-Goals

- Do not persist provider wire state in Den.
- Do not let provider wire state route messages, actions, tools, sessions, or
  wakes.
- Do not make Rust understand OpenAI Responses item schemas.
- Do not expose opaque state to model-callable tools as a coordination side
  channel.
- Do not create implementation tasks from this design until it has been
  reviewed.
