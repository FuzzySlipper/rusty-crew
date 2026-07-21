# Task 5969 Agent Switchboard Live Evidence

Certified on 2026-07-20 against the source-backed services on this host.
No bearer credentials or provider secrets are recorded here.

## Debug SQLite

- Deployment: `debug`, port `9348`, schema version `55`.
- Created `@task-5969-proof` at revision `1`, targeting the exact active pair
  `reviewer-cert-5806` / `reviewer-cert-5806-session`.
- The route resolved as routable before delivery.
- Test delivery `operator-route-test:2c488105-640e-4238-b5ee-33b9d33c8c1f`
  completed as `accepted` with `requestedAddress: @task-5969-proof` and durable
  routing provenance containing route revision `1` and the exact target.
- Restarted `rusty-crew-debug.service`; the same route and resolution survived.
- Created `@task-5969-stale` against an intentionally absent exact agent/session.
  Resolution and delivery both failed closed with
  `agent_route_direct_target_missing`.
- Calling the production route prefix on this deployment returned HTTP `409`
  with `coordination_deployment_role_mismatch`.

## Production PostgreSQL

- Deployment: `production`, port `9347`.
- Restarted `rusty-crew.service` through PostgreSQL schema migrations `38` and
  `39`; health returned `ok` without degradation.
- Created `@task-5969-proof` at revision `1`, targeting the exact active pair
  `reviewer` / `reviewer-session`.
- The bounded test delivery completed as `accepted` and persisted the requested
  address, route key, revision, and resolved concrete target.
- Created `@task-5969-managed-proof` at revision `1`, targeting exact active
  Codex binding `external-binding-47e11f9eb48db480e9a8994f` at binding revision
  `3`, with required runtime `codex_app_server` and delivery policy
  `immediate_steer`.
- Correlated round `round:92f2d1d8-a476-44a9-919c-f9d5f4397a05` reached the
  managed agent and completed as `replied` with the exact body
  `task-5969-managed-reply`.
- Its delivery receipt retained `@task-5969-managed-proof`, route revision `1`,
  the concrete external agent/session, runtime ID, binding ID/revision, and
  delivery policy.

All four temporary certification routes were deleted through their role-bound
admin APIs after evidence capture.

## Final Read-Model Proof

After the final native build, both `rusty-crew-debug.service` and
`rusty-crew.service` restarted healthy without degradation. A disposable debug
route `@task-5969-final-proof` accepted delivery
`operator-route-test:32615954-c6f0-4044-8031-0db4d61a76e2`. A subsequent route
GET returned `lastDelivery` with route revision `1`, status `accepted`, and its
terminal timestamp. The disposable route was deleted immediately afterward.

## Review-Finding Closure

Re-certified on 2026-07-21 after addressing `R5969-1` and `R5969-2`.

- Deterministic engine coverage now creates two active sessions with the same
  agent ID, deliberately routes to the session that agent-only selection would
  not choose, and proves the direct wake event names the resolved session.
- The managed-runtime variant proves the exact session and binding are retained
  for both immediate activation and a queued `serial_next_turn` promotion.
- Route activation revalidates the frozen session/profile/runtime/binding target
  and fails closed with `agent_route_activation_target_changed` if it drifts.
- Collision coverage now exercises raw-agent-first and route-first creation.
  Startup hydration also rejects a deliberately persisted route/raw-agent
  collision instead of allowing a shadowed address.
- After rebuilding the native bridge, both installed services restarted with
  `health: ok`. On the debug service, `@task-5969-rereview-proof` targeted
  `gb-run-20260719-052209-6e47afd3-coding-leased-dag-queue-rust-rusty-crew-native-longcat-2-0-benchmark`
  while another active session had the same agent ID. Delivery
  `operator-route-test:29f0ab0c-dd1d-4df9-acc7-11540844b060` returned
  `direct_brain_wake_requested` for that exact target session. The route was
  deleted immediately afterward.
