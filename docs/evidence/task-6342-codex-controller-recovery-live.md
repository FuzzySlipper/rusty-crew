# Task 6342: Codex Controller Recovery Live Certificate

Date: 2026-07-28

## Scope

Prove that the debug Rusty Crew service recovers after its attached Codex
app-server restarts without restarting Crew or changing persisted external
bindings.

## Environment

- Crew service: `rusty-crew-debug.service`, `http://127.0.0.1:9348`
- Codex service: `codex-app-server.service`
- Runtime: `rv-live-codex-5516`
- Codex version: `0.145.0`

## Procedure

1. Restart `rusty-crew-debug.service` to load the task implementation and wait
   for the external runtime to report `ready`.
2. Record the Crew PID and a SHA-256 hash over the sorted binding identity,
   runtime, status, and native-thread fields.
3. Restart only `codex-app-server.service`.
4. Poll `GET /v1/external-runtimes` until recovery reaches a terminal state.
5. Recompute the Crew PID and binding hash.

## Observed Recovery

The controller diagnostics progressed through:

```text
03:55:37 recovery attempt started after app-server disconnect
03:55:43 driver=ready recovery=succeeded attempts=1
```

- Crew PID: `308068` before and after
- Binding hash before: `b21d10533a000ae432120f60e60556167870c68444d145ae599d2bf063e53aba`
- Binding hash after: `b21d10533a000ae432120f60e60556167870c68444d145ae599d2bf063e53aba`
- Compatibility state: `certified`
- Controller generation: `183`
- Required probe result: all steps passed
- Binding resume failures: none
- Recovery consecutive failures: `0`
- Recovery next attempt: `null`

An initial certificate run exposed a durable event-ID collision because a new
driver resets its transport sequence while same-holder lease renewal preserves
the controller generation. The final implementation includes a per-connection
identity in event and raw-detail IDs; the repeated certificate passed after
that fix.

## Result

PASS. A Codex app-server process replacement recovers automatically through a
single bounded controller attempt. Crew remains running and persisted binding
identity is unchanged.
