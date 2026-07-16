# Task 5846 session resource limits live evidence

Date: 2026-07-15 PDT

## Scope

This certification exercised the debug Rusty Crew service at
`http://127.0.0.1:9348`, backed by its dedicated SQLite database. It used the
real `tester-chat` provider and a disposable `full_agent` profile.

## Result

The live smoke created
`resource-limits-cert-mrn0cf83-scoped-session` with:

- `workdir`: `/home/dev/rusty-crew/ts/packages/brain-island`
- `maxDurationMs`: `120000`
- `maxDelegationDepth`: `0`

The API returned the effective limits and the session's inherited tool
profile. Runtime diagnostics reported the same limits before and after a
`rusty-crew-debug.service` restart. A second session created without
`resourceLimits` retained the normal unset workdir behavior. Blank and
relative workdirs were both rejected by the native session boundary.

After restart, a real provider turn called the `terminal` tool without a cwd
override. The durable chat events contained a successful
`tool_call_completed` event and the assistant completed with:

```text
WORKDIR=/home/dev/rusty-crew/ts/packages/brain-island
```

The disposable profile and its sessions were hard-deleted by the smoke.

## Command

```bash
npm run smoke:session-resource-limits-live-debug-service -w @rusty-crew/brain-island
```
