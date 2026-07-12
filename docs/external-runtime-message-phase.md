# External Runtime Message Phase

Codex app-server may emit several `agentMessage` items during one turn. Rusty
Crew preserves the native message presentation phase as the optional
`messagePhase` field on external thread item snapshots and normalized event
payloads.

The browser contract permits:

- `commentary`: interim progress or analysis suitable for a secondary stream;
- `final_answer`: the assistant message intended as the terminal presentation;
- `unknown`: Codex supplied a non-null phase that this pinned Crew contract does
  not recognize;
- absent: Codex supplied no phase, including legacy items and text deltas.

Consumers must not infer phase from message order, item completion, or proximity
to a tool call. In particular, `item/agentMessage/delta` in the pinned Codex
protocol does not carry phase, so its normalized delta event intentionally omits
`messagePhase`. Full `item/started` and `item/completed` agent-message events
preserve the supplied phase.

`turn/completed` defines turn terminality. `messagePhase: final_answer` defines
message presentation semantics. One does not substitute for the other: a client
should finish turn-level loading from the turn lifecycle while using message
phase to decide whether text is commentary or the final answer.

Immediately after native thread creation, Codex can briefly reject a turnful
read because the first user message is not materialized yet. Crew retries that
exact pinned-protocol condition with `includeTurns: false` and returns the typed
thread metadata with an empty turn list. Once materialized, normal reads expose
the in-progress commentary phase. Other native read failures are not swallowed.

The contract source is
[`external-runtime-api-contract.ts`](../ts/packages/brain-island/src/external-runtime-api-contract.ts),
and the generated browser artifact is
[`external-runtime-api-v0.openapi.json`](external-runtime-api-v0.openapi.json).

## Live Proof

Debug thread `019f564d-6d32-7812-ac90-97ec7b8762e6` produced the following
normalized sequence on 2026-07-12:

1. `item/started` and `item/completed` with `messagePhase: commentary`;
2. phase-less text deltas for that same item;
3. plan updates and a completed shell-command item;
4. `item/started` and `item/completed` with `messagePhase: final_answer`;
5. phase-less text deltas for the final item;
6. `turn/completed` with status `completed`.

After restarting debug Crew, the thread snapshot retained one commentary message
and one final-answer message with no duplicate content. The test binding and
native history were archived through Crew after certification.

A separate immediate-read proof on thread
`019f5652-1c67-7dc0-95c3-976ca3c5052d` returned HTTP 200 with typed thread
metadata and an empty turn list during the pre-materialization window, then
returned one `final_answer` item after completion.
