# Task 6517 Built-In Help Live Certification

Date: 2026-08-01 (America/Los_Angeles)

This packet certifies the immutable built-in `rusty-crew` skill through real
native Crew brain turns and the rendered Rusty View client. Both runs targeted
the isolated SQLite debug service at `http://127.0.0.1:9348`; the PostgreSQL
live service at port `9347` was not changed or restarted.

## Chat Completions

- Source profile: `task-6517-chat`
- Isolated profile:
  `task-6517-chat-live-task-6517-immutable-built-in-crew-help-task-6517-w0-r0-msbet5xu`
- Provider: `kimi-k2.7`
- Brain: `chat-completions`
- Local tool profile: `basic_chat`
- Selected local tool count: `1` (`rusty_crew_help`)
- Broker packet:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T061830.406529151Z-2489964/run-index.json`
- Rusty View packet:
  `/home/dev/rusty-view/dist/.playwright/rusty-view-e2e/2490845/live-task-6517-built-in-he-b041d-in-Crew-help-task-6517-live-chromium/live-artifacts/evidence-packet.json`
- Screenshot inspected: `05-assistant-complete.png` in that artifact directory.

The rendered transcript showed `rusty_crew_help COMPLETED`, then explained the
native Crew harness, explicit `/new`, queryable command registry, and
`github.com/FuzzySlipper/rusty-crew`. Rusty View recorded 67 events, a completed
assistant message, reasoning/tool/text blocks, no page errors, and only normal
development-server console messages.

## Responses

- Source profile: `task-6517-responses-proxy`
- Isolated profile:
  `task-6517-responses-proxy-live-task-6517-immutable-built-in-crew-help-task-6517-w0-r0-msbeuih9`
- Provider: `responses-proxy-cert-5389` (live local den-router Responses route)
- Brain: `openai-responses`
- Local tool profile: `basic_chat`
- Selected local tool count: `1` (`rusty_crew_help`)
- Broker packet:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T061934.260199699Z-2494278/run-index.json`
- Rusty View packet:
  `/home/dev/rusty-view/dist/.playwright/rusty-view-e2e/2495087/live-task-6517-built-in-he-b041d-in-Crew-help-task-6517-live-chromium/live-artifacts/evidence-packet.json`
- Screenshot inspected: `05-assistant-complete.png` in that artifact directory.

The rendered transcript showed `rusty_crew_help COMPLETED`, followed by a
completed answer containing the required native Crew, `/new`, command registry,
and source-repository details. Rusty View recorded 21 events, tool/text blocks,
no page errors, and only normal development-server console messages.

## Lazy Prompt Proof

The debug provider-request cache was inspected for both turns.

First request detail ids:

- Chat Completions: `providerdbg_dbb3d148107f3e61a4570b5b`
- Responses: `providerdbg_c53ab6826f77c69cf9c8451c`

Both first requests contained the small `# Rusty Crew Harness` pointer and the
`rusty_crew_help` tool schema. Neither contained body-only markers such as
`## Profiles And Providers`, `Skills are guidance, not executable authority.`,
or the production/debug service explanation.

Second request detail ids:

- Chat Completions: `providerdbg_ddf3692259dad1b559a30054`
- Responses: `providerdbg_3685eda16dbebd623c8b33d3`

All body-only markers appeared after the successful tool result. This proves
the full built-in body is loaded on demand rather than embedded in every
provider request.

`GET /v1/admin/diagnostics/built-in-skills` also reported one registered
immutable skill, `promptPointer.available: true`, `bodyEmbedded: false`, and
separate SHA-256 fingerprints for the 293-character pointer and versioned skill
body after the final debug-service restart.

## Supporting Checks

- `npm run verify:offline`
- `npm run smoke:skills-tools`
- `npm run smoke:profile-loading`
- `npm run smoke:profile-role-assembly`
- `npm run smoke:tool-profile-selection`
- `npm run smoke:admin-diagnostics-api`
- `npm run smoke:tool-registry`
- `npm run smoke:tool-registry-parity`
- `cargo test -p rusty-crew-core-tool-registry`

## Residual Risk

An attempted direct OpenAI OAuth run using provider alias `gpt` reached the
real OAuth provider path but failed with HTTP 401 because the debug service's
stored refresh token had already been consumed. The failed run is preserved at:

`/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T061854.359308300Z-2491659/run-index.json`

That credential-state failure is separate from built-in skill selection and
execution. The successful live Responses proxy run certifies the same
production `openai-responses` brain and neutral Crew tool bridge without using a
fake provider.
