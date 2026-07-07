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
npm run smoke -- --list --lane offline
npm run smoke -- --list --lane live-provider
npm run smoke -- --list --tag memory
npm run smoke -- brain
npm run smoke -- adapter-den:default
```

The runner reads root `smoke:*` aliases and package-local smoke scripts, then
adds category, execution lane, tag, and environment-requirement metadata.
Important historical aliases such as `npm run smoke:brain`,
`npm run smoke:den`, `npm run smoke:bridge-wake`,
`npm run smoke:delegated-slice`, and `npm run smoke:mid-turn` remain valid
during the transition.

Architecture boundary work should also run:

```sh
npm run smoke:architecture-boundaries
```

That alias runs the Rust crate dependency firewall, TypeScript package boundary
check, and storage-scope ownership check together.

The default CI smoke selection is guarded by:

```sh
npm run smoke:validation-audit
```

That audit inspects the `verify:ts` smoke aliases and fails if the deterministic
offline gate starts depending on Den, local routers, service startup,
PostgreSQL, Rusty View, Telegram, OpenAI OAuth, or live providers. It also
prints catalogue counts so validation drift is visible without forcing every
historical smoke into CI.

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

## Execution Lanes

Use lane filters when deciding what can run in CI, what requires local service
roots, and what belongs to live deliverable certification.

| Lane                       | Meaning                                                                  |
| -------------------------- | ------------------------------------------------------------------------ |
| `offline`                  | Pure deterministic smoke; no service, native addon, Den, provider, or UI. |
| `native-offline`           | Deterministic but requires a built native addon or Rust bridge fixture.   |
| `local-service`            | Requires or starts a local Rusty Crew service path.                       |
| `debug-service`            | Targets the disposable debug service/root rather than durable live state.  |
| `local-infrastructure`     | Requires local infra such as Den services, den-router, or Postgres.       |
| `live-provider`            | Requires a real model/provider/token path or external live adapter.       |
| `rusty-view-certification` | Requires Rusty View/browser evidence and live-test artifact inspection.   |

The CI/offline gate is `npm run verify:offline`. It intentionally runs
deterministic Rust tests, TypeScript unit tests, boundary smokes, runtime-config
parity, and bridge validation. It must not require the live service root,
debug-service root, Den, PostgreSQL, Rusty View, Telegram, or a real provider.
`npm run smoke:validation-audit` enforces that expectation for the smoke aliases
called by `verify:ts`; native bridge checks are allowed because they are
deterministic and build local artifacts.

Offline cassette-backed checks are allowed in this gate when they validate only
committed, redacted fixture artifacts. They preserve response-shape evidence
from external systems without making CI depend on those systems being reachable.
The cassette home and refresh/redaction procedure are documented in
`fixtures/external-cassettes/README.md`.

Use `npm run smoke -- --list --lane <lane>` to find checks by execution
environment. Do not treat a lane listing as a command to run everything in that
lane blindly; some lanes contain expensive or operator-facing checks that should
be selected by task relevance.

## Live Certification

Deterministic checks are necessary before handoff, but they do not certify
rendered chat behavior. Work that changes streamed transcript rendering,
reasoning/tool/command/debug blocks, profile/session selection, browser controls,
or user-visible Rusty View behavior requires Rusty View live certification.

Use Rusty View's live testing process in
`/home/dev/rusty-view/docs/live-testing.md` through the Den Playwright broker.
The debug service at `/home/system/rusty-crew-debug` is the normal backend for
testing noise; the live service at `/home/system/rusty-crew` is for durable
agents and should not be used for broad smoke churn unless the task is
explicitly about the live deployment.

## External Cassettes

Use external cassettes when an integration shape is learned from live Den,
provider, Telegram, or Rusty View traffic but can be validated deterministically
after redaction. Cassettes live under `fixtures/external-cassettes/<system>/`
and should be consumed by package-local smoke scripts named with `cassette` so
the runner classifies them as offline fixture checks.

Good cassette candidates:

- response envelopes from Den successor Gateway routes;
- provider response item shapes after prompts and secrets are removed;
- Rusty View API readback shapes that do not require rendered browser proof;
- adapter webhook payloads with identifying text normalized.

Current external cassette families:

- `den-successor-gateway/conversation-readback` validates Den successor Gateway
  health/runtime/delivery/conversation readback shapes.
- `rusty-view-chat-api/roleplay-turn-readback` validates Rusty View chat API
  session/context/event/tool-debug readback shapes captured from the debug
  service after a real roleplay narrator turn.

Do not use cassettes as a replacement for live certification when behavior is
rendered, streamed, stateful, or model-visible. A substantial chat/runtime
change still needs the live evidence packet from
`docs/live-deliverable-certification.md`.

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

As of task 4510, the discoverable catalogue contains 273 smoke entries: 130
root aliases and 143 package entries. The default validation gate does not run
that full catalogue. It runs only the curated deterministic subset in
`verify:ts`, and the validation audit blocks accidental promotion of
live/service/infrastructure smokes into that subset.

## Adding A New Check

Choose the narrowest layer that proves the behavior:

1. Pure deterministic logic belongs in `ts/packages/<package>/test/*.test.ts`
   and should run through `npm run test:unit`.
2. Package-local integration smokes belong in
   `ts/packages/<package>/smokes/*.ts`.
3. Cross-package or operator smokes belong in `ts/smokes/*.ts`.
4. Redacted external response-shape checks may use fixtures in
   `fixtures/external-cassettes/` and package-local `cassette` smokes.
5. Live rendered chat behavior belongs in Rusty View live certification, with
   evidence recorded using Rusty View's `docs/live-testing.md` packet format.

Do not add new smoke files under package `src/`. Existing `src/smoke-*.ts`
files remain supported until they are moved in focused follow-up patches.
`npm run smoke:architecture-boundaries` enforces the current brain-island
legacy `src/smoke-*.ts` ceiling so new files do not quietly land in production
source directories. It also checks package-local `smokes/` imports, with only
explicit legacy exemptions for old smoke files that still need migration.

Prefer adding a package-local `smoke:<name>` script plus runner metadata instead
of adding another root alias. Root aliases are reserved for high-muscle-memory
commands or CI entry points, and the validation audit freezes the current root
alias count until old aliases are deliberately retired.

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
