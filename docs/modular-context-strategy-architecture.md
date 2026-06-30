# Modular Context Strategy Architecture

Status: design baseline for task #3837.

## Problem

Rusty Crew currently has useful context pieces, but they are not yet shaped by a
single first-class strategy boundary:

- Rust owns session wake state and `historyWindow.maxMessages`.
- TypeScript assembles profile role context, session-memory context, and the
  brain wake request inputs.
- `/v1/chat/sessions/{session_id}/context` reports a browser-safe estimate, but
  the estimate is diagnostic-only and does not drive wake shaping.
- Runtime maintenance can compact `session_memory_records`, but that is not the
  same as automatic model-context compaction for long chats.

The next step is to make context handling swappable without threading one
compaction approach through service-host, brain modules, and Rust persistence.

## Goals

- Put model-facing context shaping behind a small strategy interface.
- Keep the wake assembly hook centralized.
- Allow profile/session config to select strategy and thresholds.
- Emit view-visible context and compaction status without feeding those debug
  events back into model context by default.
- Preserve raw transcript/history rows; compaction creates derived artifacts.
- Leave room for responses-brain strategies without coupling the runtime to one
  provider loop.

## Non-Goals

- Do not delete or rewrite raw chat transcript rows as part of compaction.
- Do not make Den, channels, or Rusty View own context policy.
- Do not require a real tokenizer before strategy boundaries exist.
- Do not make UI/debug events implicitly model-facing.
- Do not bake roleplay lore, Den memory, or pi-agent behavior into the generic
  strategy contract.

## Ownership Boundary

Rust remains authoritative for deterministic coordination:

- session registry and status;
- body-state projection;
- queued-message TTL and no-resurrection behavior;
- durable transcript, memory, and compaction artifact persistence;
- stable event and API contracts.

TypeScript owns provider/brain-facing context composition:

- strategy registry and strategy selection;
- rendering profile, skill, memory, and summary sections into brain input;
- provider/model-specific estimate helpers until exact Rust estimators exist;
- LLM-backed summarization through modular brain/context implementations;
- adapter/view projection of safe context status.

The boundary should be narrow: Rust supplies stable refs, persisted records, and
budget inputs; TypeScript chooses how to assemble model-facing context and when
to request compaction.

## Wake Hook

The central hook is `dispatchWakeForSession` in
`ts/packages/brain-island/src/service-host.ts`.

Current high-level flow:

1. Load runtime session and registered brain.
2. Load profile context.
3. Build optional session-memory prompt context.
4. Build profile role assembly.
5. Ask Rust bridge to build a brain wake request.
6. Wake the brain and project observed events to chat/SSE.

The strategy seam should sit between profile loading and
`buildBrainWakeRequestForSession`:

```text
profile/session/provider state
        |
        v
ContextStrategyRegistry.select(policy.strategyId)
        |
        v
strategy.prepare(input) -> ContextStrategyOutput
        |
        v
buildProfileRoleAssembly(...strategy role ingredients...)
        |
        v
bridge.buildBrainWakeRequestForSession(...)
```

This avoids scattering context policy across `/context`, `/model`, session
memory rendering, and individual brain modules.

## Strategy Input

A strategy receives a frozen wake-time input object. It should be rich enough to
avoid reaching back into service-host globals:

- `session`: session id, agent id, profile id, kind, status, active branch id.
- `profile`: profile id, prompt refs/content, skill metadata, tool policy,
  local tool profile id, memory config.
- `providerBudget`: provider alias, protocol, model id, context window, max
  output tokens, reserved response tokens, safety margin, estimate quality.
- `policy`: strategy id, enabled flags, compaction thresholds, debug visibility,
  model-facing inclusion policy, strategy-specific config.
- `transcriptRefs`: branch id, head message id, recent message/slot refs,
  cursor/window metadata.
- `memoryRefs`: dense profile memory refs, session memory refs, Den memory
  guidance refs when enabled.
- `existingArtifacts`: latest context compaction artifacts relevant to the
  session/branch.
- `currentEstimate`: sampled prompt/input estimate and remaining budget.
- `capabilities`: available estimator ids, compaction implementation ids, brain
  backend/protocol capabilities.

The input may contain bounded content where already loaded, but large transcript
or lore bodies should prefer refs plus strategy-owned fetch limits.

## Strategy Output

A strategy returns only explicit products:

- `roleAssemblyInput`: model-facing sections/instructions/messages to pass into
  role assembly.
- `modelMessages`: optional direct model-facing message list override for brain
  modules that support it.
- `contextEstimate`: estimate after strategy shaping.
- `selectedRefs`: transcript, memory, lore, and artifact refs used.
- `debugEvents`: view-visible context events, each marked model-facing or
  UI-only.
- `compactionRecommendation`: none/requested/deferred, reason code, threshold
  evidence, desired target budget.
- `artifactWrites`: derived summaries or artifact write intents created by the
  strategy, if any.
- `diagnostics`: safe warnings/errors for `/context`, `/model`, admin surfaces,
  and smokes.

The output must not mutate runtime state directly. Service orchestration persists
artifacts and emits events so durability and visibility stay consistent.

## Initial Strategies

`recent_window`

- Default compatibility strategy.
- Uses current Rust pending-message/history-window behavior plus existing role
  assembly.
- No automatic compaction.

`session_memory_augmented`

- Adds Rust-selected session-memory prompt context.
- Keeps current `sessionMemoryPrompt` behavior but moves it behind the strategy
  boundary.
- Useful as the first proof that strategies can swap context inputs.

`rolling_summary_compaction`

- Uses context-fill thresholds to request or run compaction.
- Produces derived summary artifacts and selects recent transcript plus prior
  summaries for future wakes.
- Preserves raw transcript rows.

Future strategies can specialize for roleplay lore, responses API continuation
state, exact tokenizer support, or provider-specific prompt caching.

## Policy Shape

Context policy should be DB-backed and readable/editable through admin APIs.
Suggested minimum:

```json
{
  "enabled": true,
  "strategyId": "recent_window",
  "autoCompactionEnabled": false,
  "compactAtPercent": 80,
  "targetPercentAfterCompaction": 55,
  "maxContextPercentForWake": 95,
  "debugVisibility": "status",
  "includeDebugEventsInModelContext": false,
  "strategyConfig": {}
}
```

Percent values are whole percentages from 1 to 100. A strategy may reject values
that are internally inconsistent, such as target greater than trigger.

## Event Visibility Lanes

Rusty View needs context status in the visible timeline, but those events should
not automatically become model input. Use explicit event lanes:

- `model`: user/assistant/tool content selected by the strategy.
- `ui_status`: visible in chat timeline, not model-facing by default.
- `debug`: available in details drawers/admin surfaces, hidden from ordinary
  chat unless requested.
- `audit`: durable operator history; safe refs rather than large raw content.

Proposed chat event kinds:

- `context_status`
- `context_compaction_started`
- `context_compaction_completed`
- `context_compaction_failed`

Common payload fields:

- `wake_id`
- `strategy_id`
- `estimate_quality`
- `fill_percent`
- `compact_at_percent`
- `target_percent_after_compaction`
- `artifact_id`
- `reason_code`
- `model_facing`

`model_facing` defaults to `false` for all status/debug events.

## Compaction Artifacts

Compaction should create durable derived artifacts, not edit source transcript.

An artifact should record:

- artifact id and strategy id;
- session id, branch id, provider/model metadata;
- source refs and coverage range;
- estimate before and after;
- summary or structured content;
- model-facing eligibility;
- created/updated timestamps;
- failure/degraded reason when applicable.

Artifacts may initially be represented as session memory records when their
shape fits. If roleplay lore or responses-provider state needs different
semantics, add a dedicated module/table rather than overloading session memory.

## Responses Brain Migration Notes

Responses API support may need strategies that treat provider state as more than
plain prompt text:

- previous response ids or continuation state;
- provider-side item ids;
- cached input segments;
- tool call/result item grouping;
- exact usage from provider responses.

The generic strategy contract should therefore avoid assuming every brain takes
one flat prompt string. `modelMessages` and selected refs should be protocol
neutral, and responses-specific state should stay in strategy/provider metadata
rather than core session fields.

## Glossary

Context estimate

: A measured or approximate count of model-facing input tokens plus remaining
budget.

Context budget

: Provider/model limit after reserving output tokens and safety margin.

Compaction trigger

: A policy condition, usually context-fill percent, that requests summarization
or context reduction.

Compaction artifact

: Durable derived summary/metadata created from source transcript or memory
refs. It never replaces the raw source rows.

Debug event

: A UI/admin-visible status event for context decisions. It is not model-facing
unless explicitly selected by a strategy.

Model-facing event

: Content intentionally included in the next brain/model input.

Strategy

: A registered implementation that selects, summarizes, and renders context for
a wake according to policy.
