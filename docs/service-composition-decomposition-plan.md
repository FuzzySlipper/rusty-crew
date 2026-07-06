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

## Validation Pattern

Every route-family extraction should add or extend a unit test in
`ts/packages/brain-island/test/service-routes.test.ts` or a sibling route test.
For service-host migrations, keep one smoke proving the mounted HTTP route still
returns the same envelope shape.
