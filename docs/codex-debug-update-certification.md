# Codex Debug Update And Certification

Rusty Crew stages frequent Codex CLI updates through the isolated debug
deployment before live promotion. The operator command is intentionally fixed
to these resources:

- Crew: `rusty-crew-debug.service` at `http://127.0.0.1:9348`
- app-server: `codex-app-server.service`
- `CODEX_HOME`: `/home/system/rusty-crew-debug/codex-home`
- socket: `/run/user/1001/codex-app-server/app-server.sock`

It cannot be pointed at the live service. The workflow records the live
app-server PID before mutation and fails if that process changes.

## Operator Command

Choose the update action explicitly:

```bash
npm run codex:debug:update-certify -- --update
```

To certify the already-installed CLI without invoking its updater:

```bash
npm run codex:debug:update-certify -- --skip-update
```

The command reports installed and running versions before mutation, restarts
only the debug app-server and debug Crew units, waits for service health and the
Rust-owned compatibility probe, and then runs the real external-runtime smoke.
That smoke covers streamed text and reasoning, local command/file tool events,
Crew dynamic tools, control and interrupt, history readback, and exact
native-thread resume. Native threads introduced by the run are deleted before
certification.

Only a fully passing run calls
`POST /v1/admin/external-runtime-certifications`. Evidence is written atomically
with mode `0600` beneath:

```text
/home/system/rusty-crew-debug/evidence/codex-compatibility/
```

The certification ID and idempotency key derive from the exact CLI version,
consumed contract revision, probe suite revision, and workflow revision. Running
the workflow again for the same identity replays the same certification instead
of manufacturing duplicate active records. Failed runs write a separate
actionable evidence packet and never certify.

## Installation Limitations

`codex update` follows the installation method of the Codex executable at
`/home/agent/.npm-global/bin/codex`. Package-manager permissions, registry
availability, or an installation layout that does not support self-update can
make that step fail. Such a failure is contained before any service restart;
repair or update the installation using its package manager, then rerun with
`--skip-update` to certify the installed version.

Updating the executable does not replace the already-running live app-server
process. Live promotion is a separate guarded operation; do not restart
`codex-app-server-live.service` as part of this workflow.

## Guarded Live Promotion

After the exact installed identity has an active debug certification, promote
it with:

```bash
npm run codex:live:promote -- --promote
```

The command refuses active turns and unresolved interactions, snapshots exact
binding, native-thread, and turn identities, restarts only the live app-server
and live Crew units, and proves a fresh controller lease resumed the same work
without replay. Known stale native threads are reported separately. An operator
may bypass the active-work refusal only with the conspicuous
`--override-active` flag after reviewing the listed IDs.

Promotion evidence is stored under
`/home/system/rusty-crew/evidence/codex-promotions`. Failure guidance never
downgrades the CLI, replaces threads, or replays work automatically.

## Protocol Baseline Versus Installed Version

The generated app-server protocol tree is a reviewed development baseline, not
an operational pin that must equal the installed CLI version. A routine Codex
update is accepted when the Rust-owned consumed-contract probe and live smoke
pass for the exact installed version. Do not edit
`CODEX_APP_SERVER_PROTOCOL_VERSION` or regenerate artifacts solely to make a
version string match.

Regenerate and review the protocol baseline only when Crew deliberately adopts
new native methods/shapes, or when certification identifies a concrete consumed
contract incompatibility that requires source changes. The first proof of this
policy is [the Codex 0.144.3 certification](codex-app-server-0.144.3-live-certification.md),
which retained the `0.144.1` generated baseline unchanged.
