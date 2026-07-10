# Live Chat Rust Authority Certification (#5387)

Date: 2026-07-10

## Substrate

- Rusty Crew commit under test: `b4ac3d63ec612cc73370a0f39b3f830199d5abbd`
- debug service: `http://127.0.0.1:9348`
- service root: `/home/system/rusty-crew-debug`
- storage: SQLite, schema version 33
- rendered client: current `/home/dev/rusty-view` through the Playwright broker
- real providers: `tester-chat` for refresh/reasoning rendering and
  `responses-proxy-cert-5389` for the tool/mutation/replay proof

The debug service was restarted from the current checkout before certification.
`/v1/admin/healthz` was healthy and storage diagnostics reported the expected
SQLite backend.

## Live API And Stream Proof

Reusable command:

```bash
RUSTY_CREW_DEBUG_ADMIN_BASE_URL=http://127.0.0.1:9348 \
RUSTY_CREW_CHAT_CERT_PROVIDER_ALIAS=responses-proxy-cert-5389 \
npm run smoke:chat-rust-authority-live-debug-service \
  -w @rusty-crew/brain-island
```

Observed result:

- 54 live SSE events;
- successful real `git_status` and `read_file` tool calls;
- one and only one `assistant_message_completed` terminal event;
- exact 50-event reconnect suffix matched between SSE and the chat events API;
- alternate variant inherited primary branch lineage and became active;
- branch create/select, snapshot create, and snapshot jump succeeded;
- attachment and data-bank scope create/read/remove succeeded;
- the disposable certification profile was hard-deleted afterward.

The script exercises official HTTP paths and leaves no provider or profile
fixture in committed service configuration.

## Rendered Rusty View Proof

### Refresh And Reasoning

Broker run:

`/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260710T124535.162058069Z-85762/run-index.json`

Artifacts:

`/tmp/rusty-view/playwright-output/86475/live-scroll-and-refresh.li-4ad40--agent-refresh-conversation-chromium/live-artifacts`

The run passed. I inspected `refresh-before-reload.png` and
`refresh-after-reload.png` plus their debug snapshots. The same user and
assistant message ids, terminal text, and cursor `:114` remained after reload;
the client reconnected without another send or manual refresh. The rendered
assistant row included a collapsed `REASONING` control, and the event inspector
showed the reasoning delta sequence. Page errors were empty.

### Tool Activity Attachment

Broker run used for rendered inspection:

`/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260710T123941.731058649Z-75053/run-index.json`

Inspected screenshot:

`/tmp/rusty-view/playwright-output/75783/live-reasoning-controls.li-bb98f-ve-agent-reasoning-controls-chromium/live-artifacts/01-profile-selected.png`

The completed assistant row visibly contained three completed tool blocks
(`git_status`, `search_files`, `search_files`) followed by one final response.
The blocks were attached to the assistant row rather than left in a typing row,
and the event inspector showed paired start/completion events.

## Backend Parity

The same Rust repository contracts and route projections passed the live
PostgreSQL conformance suite before this certification:

```bash
RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL="$RUSTY_CREW_TEST_DATABASE_URL" \
npm run test:postgres-backend
```

All 23 PostgreSQL backend tests passed, including the shared conversation,
exact-page, lineage, transcript, attachment, and data-bank contract. The
rendered disposable test intentionally ran on SQLite; no API or semantic
difference was observed between the backend contracts. SQLite remains the
single-writer debug deployment, while PostgreSQL remains the long-lived service
backend.

## Follow-Ups Found

- Rusty View #5561 tracks a live-fixture race where a fast terminal event
  replaces an ephemeral `pending-assistant-*` id before the fixture checks its
  locator. Crew persisted and rendered the correct durable turn.
- Rusty Crew #5562 tracks two `tester-chat` tool-heavy prompts that stalled
  before their first provider delta. The same Crew tool loop passed through the
  Responses proxy provider.
- The direct `gpt` provider returned HTTP 401 during this run and needs operator
  OAuth renewal before it can be used for another direct-provider certificate.

These follow-ups do not alter the certified Rust chat mutation/read/replay
contract, but they remain explicit rather than being folded into fallback test
behavior.
