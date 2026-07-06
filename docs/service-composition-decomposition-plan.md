# Service Composition Decomposition Plan

Status: active implementation note for task 4242.

Rusty Crew's intended TypeScript boundary is:

- `service-host` owns process composition: concrete adapter injection, HTTP
  listener startup, route table composition, runtime config assembly, and
  lifecycle/drain-loop ownership.
- `brain-island` owns brain capability: pi-agent integration, brain modules,
  model-callable tools, profile/role assembly, and framework-neutral route or
  control handlers that are close to those capabilities.

The current implementation is still transitional. `service-host` starts the
socket and injects adapters, but `brain-island/src/service-app.ts` still owns
the route if-chain, many admin handlers, service drain loops, scheduler glue,
roleplay browser API, and private helper types. This is workable only while the
code remains under active remediation; new code should make extraction easier.

Before extracting a route family, classify its code into three buckets:

1. **TypeScript glue:** HTTP method/path dispatch, adapter/provider SDK calls,
   model-callable tool glue, and route-local response shaping. This may remain
   in TypeScript when kept behind explicit ports.
2. **Rust authority:** durable validation, coordination decisions, lifecycle
   state transitions, storage semantics, runtime rebuild planning, and anything
   that should be enforced rather than suggested. These pieces should move
   toward Rust crates or bridge-backed operations instead of becoming a smaller
   TypeScript authority island.
3. **Generated contract surface:** request/response envelopes, Rust/TS wire
   shapes, and API catalog metadata that can drift when hand-maintained. These
   should be candidates for bridge/OpenAPI/codegen once the shape is stable.

Extraction should reduce TypeScript authority, not merely move it to a new
file. A route module is good when it makes the remaining TypeScript look more
like code-as-config around Rust-owned state and narrow external-adapter glue.

## Target Modules

Use narrow, framework-neutral route modules first. A route module should accept
a small request shape and explicit context ports rather than `ServiceState` or
`IncomingMessage` whenever practical.

Already extracted examples:

- `service-scheduler-routes.ts`
- `service-context-strategy-routes.ts`
- `service-mcp-catalog-routes.ts`
- `service-tool-catalog-routes.ts`
- `service-local-tool-profile-routes.ts`

Next good candidates:

- `routes/model-provider-admin.ts` for provider CRUD/OAuth admin routing.
- `routes/profile-registry.ts` for profile registry write planning/apply
  routing.
- `routes/debug.ts` for direct debug context and provider-request snapshots.
- `routes/roleplay-lore.ts` before roleplay browser endpoints accrete more
  unrelated service concerns.
- `routes/admin-capabilities.ts` for route/command/catalog discovery.

Drain loops and lifecycle loops should move later, after route seams have more
unit coverage. Their target home is `service-host`, with brain-island exposing
ports and executors rather than concrete adapter imports.

## Index Split Plan

`ts/packages/brain-island/src/index.ts` is both a public barrel and a shared
type home. That makes circular imports easier to introduce.

Preferred path:

1. Move shared implementation-neutral types into explicit modules such as
   `brain-types.ts`, `service-types.ts`, and `tool-types.ts`.
2. Update internal imports to read those direct modules instead of importing
   from `index.js`.
3. Leave `index.ts` as a thin public export surface.
4. Add a boundary smoke that fails if production files under `src/` import from
   the package barrel.

## Non-Goals

- Do not move model-callable tools into `service-host`.
- Do not reintroduce concrete adapter imports into `brain-island` production
  code.
- Do not move all routes at once. Prefer one route family plus focused tests.
- Do not add a web framework solely for extraction. The current route result
  envelope is enough.
- Do not preserve compatibility with old internal module paths when a clean
  import can be updated in one pass.

## First Extraction

Task 4242 extracted local tool profile admin routing from `service-app.ts` into
`service-local-tool-profile-routes.ts`.

The handler now receives:

- `method`, `url`, and `requestId`;
- a lazy `readBody` callback for write methods;
- an explicit `LocalToolProfileStore`.

This keeps the route testable without a live service, bridge, or HTTP server.
`service-app.ts` remains the dispatcher for now and composes the real store from
`state.bridge` plus `state.now`.

Task 4325 started the second wave by moving shared route-result types into
`service-route-results.ts` and extracting static site serving into
`service-static-site-routes.ts`. That slice deliberately moved filesystem/path
serving glue only; the embedded admin panel HTML remains in `service-app.ts`
until it can move in a focused UI/static-resource slice.

Task 4332 then moved the embedded admin-panel HTML and route decision helpers
into `service-admin-panel-routes.ts`. This is intentionally low-authority
TypeScript: static HTML/resource projection, not service behavior.

Task 4333 moved model-provider CRUD and OpenAI OAuth admin routing into
`service-model-provider-routes.ts`. The new module owns HTTP path/method
dispatch, OAuth authorization/pending-login glue, credential-secret envelope
shaping, provider API read projection, and route-local error mapping through
explicit ports. It does not receive `ServiceState`.

The 4333 authority split is:

- **TypeScript glue now extracted:** route parsing, OAuth start/status/complete
  provider glue, redacted pending-login projection, decimal `temperature`
  request/readback mapping, and revision-conflict route envelopes.
- **Rust authority still visible in `service-app.ts`:** provider/profile
  runtime rebuild impact planning and profile/session refresh application. This
  remains intentionally unhidden until it moves behind a Rust/control-plane
  operation.
- **Generated contract candidates:** model-provider write/read envelopes,
  revision-conflict response shape, OAuth status/start/complete/clear request
  bodies, and the public `temperature`/`temperatureMilli` projection. These
  should become generated/OpenAPI or bridge-owned contracts rather than another
  manually maintained UI/backend handshake.

Task 4334 moved the profile-registry write route wrapper into
`service-profile-registry-routes.ts`. This is intentionally a route-shell
extraction: the module owns method/path validation, missing-record error
mapping, plan-vs-apply orchestration, and lifecycle/runtime effect dispatch
through explicit ports. It does not own profile validation, runtime-config
mutation semantics, or lifecycle authority.

The 4334 authority split is:

- **TypeScript glue now extracted:** profile registry write-route parsing,
  method/unknown-route envelopes, missing DB-backed profile mapping, and
  deciding whether a successful apply should call lifecycle or runtime-config
  effect ports.
- **Authority still visible in `service-app.ts`:** registry field/prompt/
  lifecycle next-record planning, editable runtime-config composition, local
  tool profile validation, MCP binding synthesis, profile JSON writes, runtime
  config file mutation, session archiving, and brain unregister behavior. These
  are too semantic to hide in another TypeScript module as cleanup.
- **Rust/codegen candidates:** profile registry write plans, editable
  runtime-config request/response envelopes, revision/lifecycle error shapes,
  profile/session referential-integrity checks, and runtime config replacement
  apply plans.

Task 4335 moved Rusty View chat route dispatch and SSE stream handling into
`service-chat-stream-routes.ts`. The module now owns `/v1/chat` dispatch order
(stream route first, then the existing chat API handler), stream method/session
validation, SSE serialization, replay-on-connect, heartbeat setup, subscriber
cleanup, and CORS projection for chat responses. `service-app.ts` supplies the
same chat capability ports as before.

The 4335 authority split is:

- **TypeScript glue now extracted:** chat route detection, stream route parsing,
  SSE framing, stream replay wiring, subscriber cleanup, chat CORS projection,
  and the dispatch from stream route to the existing Rusty View chat API
  handler.
- **Authority still visible outside the route module:** session identity,
  transcript/event persistence, message variant/branch mutation semantics,
  slash-command execution, wake submission, provider/tool debug detail lookup,
  and context usage estimation. Those remain behind existing service ports and
  should move only through Rust/control-plane or generated contract work, not
  as hidden route cleanup.
- **Generated contract candidates:** SSE event catalog, chat API OpenAPI
  schemas, command output envelopes, debug-detail payloads, message
  slot/variant/branch schemas, and stream cursor/replay semantics.

## Validation Pattern

Every route-family extraction should add or extend a unit test in
`ts/packages/brain-island/test/service-routes.test.ts` or a sibling route test.
For service-host migrations, keep one smoke proving the mounted HTTP route still
returns the same envelope shape.
