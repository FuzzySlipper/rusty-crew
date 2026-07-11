# CI Verification Efficiency

Status: active convention

Rusty Crew keeps two stable required GitHub check names:

- `Verify Offline`
- `Verify Postgres Backend`

The offline lane remains a single diagnostic unit while its measured GitHub
duration stays comfortably below the 8-10 minute split threshold. PostgreSQL
conformance remains separate because it has a distinct service dependency and
failure mode. Live-provider and Rusty View certification remain external
delivery evidence and are not replaced by deterministic CI.

## Native Build Ownership

`verify:ts` owns the single release native-addon build used by TypeScript unit
tests and subsequent native smokes. `smoke:bridge-native-surface` validates the
already-built declaration surface. For an isolated one-step build and check,
run:

```sh
npm run verify:bridge-native-surface
```

This preserves direct local ergonomics without rebuilding the addon twice in
the full offline lane.

## Measurements

Measurements were taken on 2026-07-11 from a clean checkout with warm Cargo and
npm caches. GitHub timings are from clean hosted-runner check results and
include setup/cache overhead.

| Environment | Revision | Verify Offline | PostgreSQL | Notes |
| --- | --- | ---: | ---: | --- |
| GitHub before | `088f01d` | 3m39s | 1m08s | Existing hosted-runner baseline |
| Local before | pre-#5531 | 72.013s | not run | Native build invoked twice; second warm build was incremental |
| Local after | #5531 working tree | 72.011s | not run | Native build invoked once |

The exact post-change GitHub durations are recorded in the task review packet
after the commit is pushed, because hosted-runner evidence does not exist until
that point.

The local warm-cache saving is expected to be small; the structural benefit is
removing redundant N-API build invocation and cold-run risk without deleting
any architecture, generated-artifact, SQLite, PostgreSQL, or native-runtime
coverage.
