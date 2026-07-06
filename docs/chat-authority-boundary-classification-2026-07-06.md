# Chat Authority Boundary Classification

This note classifies the Rusty View chat service seams after the #4335
follow-up work. It is intentionally scoped to authority placement rather than
frontend behavior.

## Rust-Owned Authority

These operations are authoritative in Rust persistence, bridge, or control-plane
ports. TypeScript may call them and shape API envelopes, but should not re-own
their mutation semantics.

| Surface | Current authority | Notes |
| --- | --- | --- |
| durable message slots and variants | `core-persistence` bridge operations | `saveMessageSlot`, `saveMessageVariant`, variant delete/reorder/select, and branch head updates are native bridge calls. |
| conversation branches and snapshots | `core-persistence` bridge operations | Chat API handlers route through `service-app` functions that call native branch/snapshot operations. |
| wake dispatch and in-flight suppression | Rust bus/body plus service wake dispatcher | Chat message submission enters the service turn path; wake execution is not route-owned. |
| session registry and lifecycle status | Rust session/config/control-plane operations | Routes resolve sessions through bridge-projected session state and must not mutate lifecycle directly. |
| profile registry mutation planning | `core-config` via native bridge | Profile registry update/lifecycle/prompt planning is now Rust-owned. |

## TypeScript Glue

These are acceptable TS responsibilities because they sit at the HTTP, adapter,
or brain boundary.

| Surface | TS responsibility | Guardrail |
| --- | --- | --- |
| HTTP envelope parsing | `rusty-view-chat-api.ts` | Keep handlers as validation plus delegation; avoid wake or persistence logic here. |
| SSE stream wiring | `service-chat-stream-routes.ts` | Owns transport replay/subscription only; event creation stays in service/runtime paths. |
| chat event projection | `service-app.ts` | Adapter projection from core/brain events into Rusty View event records until a Rust projection port exists. |
| brain/provider/tool calls | `brain-island` brain modules | Tool/provider execution remains outside core coordination. |
| command execution adapter | slash-command router plus service glue | Command registry and output shaping are TS glue; lifecycle/session mutation effects should call control-plane ports. |

## Generated Or Contract-Validated Surface

These files should be treated as the API contract catalog for frontend and CI
drift checks.

| Surface | Contract file | Smoke |
| --- | --- | --- |
| Rusty View chat API | `docs/rusty-view-chat-api-v0.openapi.json`, `rusty-view-chat-contract.ts` | `smoke:rusty-view-chat-contract` |
| profile registry admin API | `docs/profile-registry-admin-api-v0.openapi.json`, `profile-registry-admin-contract.ts` | `smoke:profile-registry-admin-contract` |
| route command/capability registry | `api-command-registry.ts` | `smoke:api-command-registry` |

## Current #4366 Movement

`POST /v1/chat/sessions/{session_id}/messages` now uses the shared chat session
resolver and delegates the durable message plus wake submission to the
`sendMessage` service port. A focused smoke verifies that:

- the route forwards the parsed actor/body/reason/idempotency data to the port,
- idempotency falls back to `client_message_id` when no header is supplied,
- archived sessions are rejected before the port is called.

This keeps the route as validation and envelope code while the service port owns
the chat message mutation and wake submission path.

## Remaining Follow-Ups

- Move chat event projection from TS helper functions into a Rust projection
  port once event schemas stabilize.
- Move slash-command lifecycle/session effects behind explicit control-plane
  command ports; keep command text parsing and autocomplete in TS.
- Decide whether attachment and data-bank mutations should become Rust-owned
  domain repos or remain TS glue around native persistence.
- Generate the TypeScript `ChatEvent` kind union from the OpenAPI contract once
  bridge/codegen ownership for API contracts is settled.
