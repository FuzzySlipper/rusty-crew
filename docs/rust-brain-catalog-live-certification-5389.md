# Rust Brain Catalog Live Certification

Status: passed for task #5389

Date: 2026-07-09

## Substrate

- Debug service: `http://127.0.0.1:9348`
- Service root: `/home/system/rusty-crew-debug`
- Storage: SQLite
- Built-in catalog: `pi-agent`, `openai-responses`
- Chat-completions provider: `tester-chat` through local den-router
- Responses provider: disposable `responses-proxy-cert-5389` through local
  den-router's live `gpt` Responses route
- Responses profile/session: `responses-cert-5389` /
  `responses-cert-5389-session`

The debug service's direct `gpt` OAuth credential was absent, so this run used
the explicit no-key proxy path. The production `openai-responses` host still
ran the Rust Responses loop with `clientMode=live`; no deterministic provider
or live/fake service toggle was involved.

## Live Provider Proofs

### OpenAI Responses

Successful tool wake:

- wake: `service-responses-cert-5389-session-1783650499256-1`
- tool: `read_file` on `/home/dev/rusty-crew/package.json`
- events: `tool_call_started`, `tool_call_completed`, streamed
  `assistant_text_delta`, `assistant_turn_finished`,
  `assistant_message_completed`
- final text reported package `rusty-crew` and marker `responses5389`

Recoverable tool-error wake:

- wake: `service-responses-cert-5389-session-1783650522895-2`
- tool: `read_file` on a deliberately missing path
- events: `tool_call_started`, `tool_call_failed`, streamed final text, normal
  completed terminal events
- final text reported `ENOENT`, did not retry, and included `recover5389`

After service restart, provider-state diagnostics reported:

- module `openai-responses`, strategy `replay`
- `clientMode=live`
- provider state `valid`
- payload version `openai-responses-state-v1`
- last wake id matching the recoverable-error wake

`POST /v1/chat/sessions/responses-cert-5389-session/commands` with `/model`
reported provider alias `responses-proxy-cert-5389`, model `gpt`, Responses
protocol, Rust brain backend `openai-responses`, context estimate, tool count,
and local tool profile.

### Pi-Agent

Successful reasoning/tool wake:

- session: `tester-session`
- wake: `service-tester-session-1783650572893-3`
- tool: successful `read_file` on `package.json`
- events included live `assistant_reasoning_delta` with format
  `chat-completions:reasoning_content`, tool start/completion, streamed text,
  turn completion, and final marker `pi5389`

### Roleplay Narrator

- session: `narrator-fsm-cert-rp-session`
- wake: `service-narrator-fsm-cert-rp-session-1783650606881-4`
- Rust-hosted phase events: `exploring`, `composing`, `idle`
- live reasoning deltas: 171
- tool events included successful scene-state reads/writes
- final output was clean narrative with no tool/protocol artifacts

## Timeout And Cancellation

The existing live debug timeout harness ran against `tester-session`:

```bash
RUSTY_CREW_TIMEOUT_LIVE_PROFILE_ID=tester \
RUSTY_CREW_TIMEOUT_LIVE_SESSION_ID=tester-session \
npm run smoke:wake-timeout-live-debug-service -w @rusty-crew/brain-island
```

Results:

- disabled timeout: completed normally in 1188 ms;
- 25 ms service cap: failed visibly with `reason_code=wake_timeout` and summary
  `timed out after 25ms`;
- 60000 ms session override: completed normally in 1185 ms;
- the harness restored the original disabled service wake-timeout policy.

A separate live provider-idle probe temporarily set the debug pi-agent stream
idle budget to 25 ms. Wake
`service-tester-session-1783650748587-2` was rejected with a browser-visible
summary containing `provider stream idle timeout`. The debug environment was
restored to 300000 ms and the service restarted healthy.

## SSE And Rusty View

A direct SSE client connected to
`/v1/chat/sessions/responses-cert-5389-session/stream`, sent wake
`service-responses-cert-5389-session-1783650867155-1`, received 33 chunks, and
observed both marker `sse5389` and `assistant_message_completed` without a
refresh/readback poll.

Rusty View's brokered Chromium live reasoning scenario passed against the same
debug service:

- evidence:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260710T023839.956590864Z-2253430/run-index.json`
- test: `reasoning-controls.live.spec.ts`
- result: one passed Chromium scenario
- visual impact: 117933 changed screenshot bytes when expanding the reasoning
  control
- screenshots under:
  `/tmp/rusty-view/playwright-output/2254549/live-reasoning-controls.li-bb98f-ve-agent-reasoning-controls-chromium/live-artifacts`

The expanded screenshot was inspected. It showed a connected service, visible
reasoning/plan content, final assistant text, and no incoherent overlap.

An additional optional Rusty View activity-template run timed out before any
provider response. Its isolated session recorded only the user message and
provider debug snapshot, so it did not provide UI tool evidence. Exact packet:

`/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260710T023959.061501737Z-2259577/run-index.json`

The forced-tool live API wakes above completed normally before and after this
run. The disposable in-flight wake was cleared by restarting the debug service;
`GET /v1/admin/diagnostics/buffered-brain-runs` then reported zero active runs
for both modules.

## Deterministic Gates

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `npm run typecheck`
- `npm run smoke:brain-catalog`
- `npm run smoke:pi-agent-rust-bridge -w @rusty-crew/brain-island`
- `npm run smoke:openai-responses-tool-bridge`
- `npm run smoke:openai-responses-cancellation -w @rusty-crew/brain-island`
- `npm run smoke:roleplay-narrator-brain -w @rusty-crew/brain-island`
- `npm run smoke:brain-island-entrypoint-surface -w @rusty-crew/brain-island`
- `npm run smoke:provider-state-fingerprints -w @rusty-crew/brain-island`
- `npm run smoke:bridge-validation`
- `node tools/check-ts-package-boundaries.mjs`
- `node tools/check-production-fakes.mjs`
