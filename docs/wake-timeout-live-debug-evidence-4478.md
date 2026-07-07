# Wake Timeout Live Debug Evidence

Task: #4478

Date: 2026-07-07

Command:

```bash
npm run smoke:wake-timeout-live-debug-service -w @rusty-crew/brain-island
```

Debug service:

- Base URL: `http://127.0.0.1:9348`
- Config path: `/home/system/rusty-crew-debug/config/service.json`
- Profile: `asha-planner`
- Session: `asha-planner-session-20260706T09140884-1`

The smoke applies runtime config drafts through
`/v1/admin/control/config/draft/apply` and restores
`wakeTimeout: { "mode": "disabled" }` in `finally`.

## Scenarios

### Disabled Service Policy

Config:

```json
{ "wakeTimeout": { "mode": "disabled" } }
```

Readback:

- `/v1/chat/sessions` omitted `effective_defaults.wakeTimeoutMs`.

Result:

- Wake: `service-asha-planner-session-20260706T09140884-1-1783395577076-1`
- Event count after cursor: 29
- Elapsed: 1273 ms
- `assistant_message_completed.status`: `completed`
- Summary: `timeout disabled ok`
- `assistant_turn_finished`: present

### Service Default Cap

Config:

```json
{ "wakeTimeout": { "mode": "default", "defaultMs": 25 } }
```

Readback:

- `/v1/chat/sessions` reported `effective_defaults.wakeTimeoutMs: 25`.

Result:

- Wake: `service-asha-planner-session-20260706T09140884-1-1783395578397-2`
- Event count after cursor: 3
- Elapsed: 45 ms
- `assistant_message_completed.status`: `failed`
- `assistant_message_completed.reason_code`: `wake_timeout`
- Summary: `wake service-asha-planner-session-20260706T09140884-1-1783395578397-2 timed out after 25ms`
- `assistant_turn_finished`: present

This scenario exposed and verified the fix for early timeout terminal events:
timeouts before any assistant stream event now still append
`assistant_message_completed` and `assistant_turn_finished`.

### Session Override

Config:

```json
{
  "wakeTimeout": { "mode": "default", "defaultMs": 25 },
  "sessions": [
    {
      "sessionId": "asha-planner-session-20260706T09140884-1",
      "turnTimeoutMs": 60000
    }
  ]
}
```

Readback:

- `/v1/chat/sessions` reported `effective_defaults.wakeTimeoutMs: 60000`.

Result:

- Wake: `service-asha-planner-session-20260706T09140884-1-1783395578500-3`
- Event count after cursor: 54
- Elapsed: 1262 ms
- `assistant_message_completed.status`: `completed`
- Summary: `timeout override ok`
- `assistant_turn_finished`: present

## Restore Verification

After the smoke:

- `/home/system/rusty-crew-debug/config/service.json` has
  `wakeTimeout: { "mode": "disabled" }`.
- `/v1/chat/sessions` omits `effective_defaults.wakeTimeoutMs` for the tested
  session.
