# Smoke Test Inventory And Runner

Status: active transition convention

Rusty Crew has accumulated a useful but noisy smoke layer. The tests catch real
integration behavior, but many live beside production source files and the root
`package.json` has grown one alias per proof. Keep the smokes; move the
organization toward a manifest-backed runner and non-production smoke homes.

## Commands

Use the discoverable runner for new work:

```sh
npm run smoke -- --list
npm run smoke -- --list --package brain-island
npm run smoke -- --list --category service-host
npm run smoke -- --list --tag memory
npm run smoke -- brain
npm run smoke -- adapter-den:default
```

The runner reads root `smoke:*` aliases and package-local smoke scripts, then
adds category, tag, and environment-requirement metadata. Important historical
aliases such as `npm run smoke:brain`, `npm run smoke:den`,
`npm run smoke:bridge-wake`, `npm run smoke:delegated-slice`, and
`npm run smoke:mid-turn` remain valid during the transition.

## Categories

Use these categories when adding or reclassifying smokes.

| Category              | Meaning                                                              |
| --------------------- | -------------------------------------------------------------------- |
| `package-integration` | Package-local integration proof without a service/live dependency.   |
| `native-bridge`       | Exercises bridge/native addon or Rust fixture compatibility.         |
| `service-host`        | Requires or validates the Rusty Crew service/admin/debug host path.   |
| `adapter-integration` | Exercises a platform adapter boundary such as MCP, Telegram, or TUI. |
| `den-adapter`         | Talks to Den successor services or Den-facing activity surfaces.      |
| `storage`             | Exercises backend-neutral storage, Postgres, or storage migration.    |
| `rusty-view`          | Proves a route or behavior intended for the Rusty View client.        |

## Environment Requirements

The runner reports requirement flags so a caller can see whether a smoke is
safe to run locally without extra setup.

| Requirement        | Meaning                                                         |
| ------------------ | --------------------------------------------------------------- |
| `none`             | No special service or credential expected.                      |
| `native-build`     | Requires the native bridge/artifacts to be built.               |
| `service-startup`  | Requires or starts the Rusty Crew service host path.            |
| `den`              | Requires Den successor services or Den-facing tokens/config.    |
| `local-router`     | Requires local den-router, usually `http://127.0.0.1:18082`.    |
| `postgres`         | Requires a Postgres-backed storage configuration.               |
| `rusty-view`       | Requires Rusty View or its live-test browser path.              |
| `live-provider`    | Requires a real model provider path, not only deterministic IO. |
| `openai-oauth`     | Requires direct OpenAI OAuth Responses provider state.          |
| `telegram-config`  | Requires Telegram adapter configuration.                        |

## Current Inventory Shape

The current smoke population is discoverable with:

```sh
npm run --silent smoke -- --list --json
```

The broad buckets at the start of this transition are:

- Root aliases preserve commonly used commands and should shrink over time.
- `ts/packages/brain-island/src/smoke-*.ts` contains most service, brain,
  context, tool, memory, command, profile, MCP, and Rusty View contract smokes.
- `ts/packages/adapter-den/src/smoke-*.ts` contains Den adapter and successor
  service proofs.
- `ts/packages/adapter-telegram/src/smoke-*.ts`,
  `ts/packages/adapter-mcp/src/smoke-*.ts`, and
  `ts/packages/adapter-tui/src/smoke-*.ts` contain adapter-local proofs.
- `ts/packages/contracts/src/smoke-*.ts` and
  `ts/packages/native-bridge/src/smoke-*.ts` contain contract and bridge
  compatibility proofs.
- `ts/smokes/*.ts` is the current home for cross-package/operator smokes.

## Adding A New Check

Choose the narrowest layer that proves the behavior:

1. Pure deterministic logic belongs in `ts/packages/<package>/test/*.test.ts`
   and should run through `npm run test:unit`.
2. Package-local integration smokes belong in
   `ts/packages/<package>/smokes/*.ts`.
3. Cross-package or operator smokes belong in `ts/smokes/*.ts`.
4. Live rendered chat behavior belongs in Rusty View live certification, with
   evidence recorded using `docs/live-deliverable-certification.md`.

Do not add new smoke files under package `src/`. Existing `src/smoke-*.ts`
files remain supported until they are moved in focused follow-up patches.

Prefer adding a package-local `smoke:<name>` script plus runner metadata instead
of adding another root alias. Root aliases are reserved for high-muscle-memory
commands or CI entry points.

## Moving Existing Smokes

When moving an existing smoke out of `src`:

1. Move it to `ts/packages/<package>/smokes/<name>.ts`.
2. Update the package-local script path.
3. Keep any important root alias pointing at the package script.
4. Run `npm run smoke -- --list` and the moved smoke.
5. If the smoke proves user-visible chat/runtime behavior, include the live
   evidence packet required by `docs/live-deliverable-certification.md`.

No useful smoke should be deleted just because it is noisy. Classify it, move it
when the owning area is touched, and convert pure logic to unit tests where that
gives a stronger signal.
