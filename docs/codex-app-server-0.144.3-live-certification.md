# Codex App-Server 0.144.3 Live Certification

Task `#5790` certified the installed `codex-cli 0.144.3` on 2026-07-14 through
the debug-first compatibility workflow and guarded live promotion introduced by
tasks `#5784` through `#5789`.

## Certified Identity

| Field | Value |
| --- | --- |
| CLI version | `0.144.3` |
| Consumed contract | `fb1ed28c8be1213d0ce6914be30b5a2b93e98c70482cdfba336dc96b72c327ee` |
| Probe suite | `codex-required-capabilities-v1` |
| Debug runtime | `rv-live-codex-5516` |
| Debug certification | `codex-debug-a0125a80c56a9cab315c53cf` |
| Live runtime | `rusty-crew-live-codex` |
| Live certification | `codex-live-be3fd3a4f322146ca2f33a21` |

Both runtime registrations reported `observedState: ready`, controller
`driverState: ready`, and `compatibilityState: certified` after the workflow.
The protocol generator remains based on the accepted `0.144.1` development
baseline because `0.144.3` passed the existing consumed contract unchanged.

## Provider And Tool Proof

The debug app-server used its private Codex home and a real provider-backed
thread. The final focused run produced:

- 9 assistant text deltas;
- 16 normalized reasoning deltas;
- 12 command-activity events and 2 file-activity events;
- a verified local file mutation followed by command readback;
- two `rusty_crew` dynamic coordination tool executions in a correlated
  Codex-to-Codex round;
- SSE replay, Plan-mode interaction resolution, steer, and interrupt controls;
- the same native thread after Crew/controller restart.

The primary native thread was
`019f5ef4-d26a-7ed0-9440-7362b8c4f5b7`; the post-restart turn was
`019f5ef6-554f-76e1-895f-b4d355f3b30e`.

## Promotion Proof

The guarded promotion observed no active turns or unresolved interactions. It
restarted only `codex-app-server-live.service` and `rusty-crew.service`, acquired
a fresh controller instance/lease, and preserved all six exact live
binding-to-native-thread mappings. Four healthy native turn histories remained
unchanged. Two bindings whose native rollout files were already missing stayed
isolated as stale; promotion did not recreate or replay them.

Promotion evidence is stored at:

```text
/home/system/rusty-crew/evidence/codex-promotions/promotion-be3fd3a4f322146ca2f33a21.json
```

## Rusty View Resume Proof

The deployed live Rusty View opened the managed `rusty-view` binding, completed
a provider-backed marker turn, reloaded the browser, recovered the same native
thread `019f55e9-1313-78c1-a07c-7f42ea6c922b`, and completed a second marker
turn. The two native turn IDs were:

- `019f5ef0-a2c1-7893-bf55-cd97b787985a` before reload;
- `019f5ef0-c9f3-7c52-b0f8-663f8dc4d9e4` after reload.

Screenshots and the browser evidence packet are under:

```text
/home/system/rusty-crew/evidence/codex-promotions/0.144.3-view-reconnect/
```

## Storage Isolation

The live service read `RUSTY_CREW_DEPLOYMENT_ROLE=production` and
`RUSTY_CREW_STORAGE_BACKEND=postgres`; the debug service read
`RUSTY_CREW_DEPLOYMENT_ROLE=debug` and `RUSTY_CREW_STORAGE_BACKEND=sqlite`.
Their persistence diagnostics differed independently: live contained 12
external bindings while debug contained 37. The debug certification read back
on port `9348` and returned `data: null` on port `9347`, proving it was not
written through the live PostgreSQL repository.

The combined task evidence is stored at:

```text
/home/system/rusty-crew/evidence/codex-promotions/codex-0.144.3-task-5790-certification.json
```

## Update Contract

Future routine CLI updates use the fixed debug certification and live promotion
commands documented in
[Codex debug update and certification](codex-debug-update-certification.md).
A version change by itself is not a source-change trigger. Regenerate the
protocol baseline only when Crew intentionally adopts new native operations or
the consumed contract no longer passes.
