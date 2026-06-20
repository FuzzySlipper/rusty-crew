# Agent Activity Observation Producer

Status: initial implementation note for task 2949

Rusty Crew emits Den observation breadcrumbs through a small TypeScript producer in `@rusty-crew/brain-island`.

The producer follows `den-services/agent-activity-observation-contract-2026-06-19` and creates display-only `agent_activity.v1` lifecycle events for:

- agent session lifecycle;
- visible work checkpoints;
- long or risky tool calls;
- adapter degraded/recovered state;
- admin/control-plane command lifecycle.

## Boundary

Observation events are operator-facing projection facts. They do not claim, retry, complete, wake, or dedupe work. Authoritative state stays with runtime state, task state, delivery, queues, and completion packets.

Payloads carry compact summaries and references:

- `work_ref` points to project/task/assignment/run/channel/session handles;
- `result_ref` points to messages, documents, commits, or artifacts;
- `agent_identity` uses canonical profile and concrete instance/session identity when known.

The producer deliberately avoids raw prompts, full tool output, secrets, full environment details, or executable commands. Admin command activity may include a control-plane command name such as `/status` as a breadcrumb handle, but it must not contain an executable shell command or full invocation payload.

## Failure Posture

The producer supports optional and required observation modes.

- Optional mode skips events when no sink is configured and returns `status: "skipped"`.
- Required mode returns `status: "degraded"` with `reasonCode: "observation_unavailable"` when no sink is configured.
- Sink write failures return `status: "degraded"` with the same reason code.

Callers should surface degraded writes through diagnostics instead of silently treating observation as healthy.

## Current Shape

Implemented helpers:

- `sessionActivity`
- `workActivity`
- `toolActivity`
- `adapterActivity`
- `adminCommandActivity`
- `createAgentActivityObservationEvent`
- `AgentActivityObservationProducer`
- `createMemoryAgentActivityObservationSink`

The current sink interface is intentionally small:

```ts
interface AgentActivityObservationSink {
  writeAgentActivity(event: AgentActivityObservationEvent): Promise<unknown> | unknown;
}
```

This keeps the producer decoupled from the eventual HTTP client or Den adapter route. A later adapter can implement the sink by posting to `POST /v1/observation/lifecycle-events` or any route alias Den Services standardizes.

## Smoke Coverage

`npm run smoke:agent-activity-observation` verifies:

- contract-shaped event envelopes and payloads;
- session, work, tool, adapter, and admin event helpers;
- summary bounding to 240 characters;
- sink write degradation;
- required missing config degradation;
- optional missing config skip behavior.
