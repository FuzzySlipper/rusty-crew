# Codex Thread Cleanup 5695

Date: 2026-07-12

This cleanup used only Rusty Crew external-runtime lifecycle APIs. It did not
edit either Codex state database, remove rollout files, or classify threads from
cwd or age alone.

The reusable operator command is:

```bash
node ops/scripts/codex-thread-cleanup.mjs \
  --base-url http://127.0.0.1:9347 \
  --runtime-id rusty-crew-live-codex
```

The command is read-only unless `--apply` is present. It pages both native
catalogs, joins Crew bindings and task references, includes source/name/cwd and
timestamps, and preserves ambiguous records. The reviewed pre-apply selection
is recorded in
[`codex-thread-cleanup-5695-manifest.json`](codex-thread-cleanup-5695-manifest.json).

## Selection

Live thread archival required all of the following:

- an exact certification marker from the script's allowlist;
- no user-assigned thread name;
- a native state other than `active`;
- no pending interaction.

The 90 selected live threads grouped by marker as follows:

| Marker | Count |
| --- | ---: |
| `EXTERNAL_SESSION_CREATED_5675` | 16 |
| `EXTERNAL_BROWSER_CREATE_OK` | 5 |
| `EXTERNAL_SERVICE_LIVE_OK` | 29 |
| `CODEX_CODEX_REPLY_OK` | 13 |
| `RV_FRESH_DIFF_COMPLETE` | 4 |
| `CAPABILITY_EDIT_OK` | 7 |
| `smoke-recipient` | 3 |
| `rusty_crew.echo_probe` | 13 |

Missing-history binding archival required an exact controller resume failure
containing `no rollout found for thread id`, absence from both native catalogs,
and no pending interaction. This selected three live bindings and 16 debug
bindings left behind when the debug Codex home was isolated.

Four currently bound live user/project threads were preserved explicitly:

- `019f55e9-1313-78c1-a07c-7f42ea6c922b`
- `019f55e0-9de9-78b3-b2f4-c1bf63d942f6`
- `019f55d4-6b08-73e2-81e3-a17f8b181613`
- `019f55c1-68fe-7ed0-9573-8f2d0c2f4d06`

All other records without an explicit marker were preserved.

## Results

| Inventory | Before | After |
| --- | ---: | ---: |
| Live default-visible threads | 242 | 152 |
| Live archived threads | 0 | 90 |
| Live archive candidates remaining | 90 | 0 |
| Live missing-history binding candidates | 3 | 0 |
| Debug default-visible threads | 1 | 1 |
| Debug stale binding candidates | 16 | 0 |
| Debug binding resume failures after restart | 16 | 0 |

The three live bindings associated with completed View task `#5675` moved to
`archived` revision 3. All 17 debug bindings are archived; the additional one
is the already archived isolation-proof binding from task `#5694`.

Thread `019f55b8-32bb-7da2-bad3-a8f4d7445e8f` was restored through Crew,
observed in the default list, and archived again. This proves the selected
history remains recoverable.

The first apply attempt also exposed a Rust validation defect: an archived Crew
session could not transition its existing external binding to `archived`.
Core now permits only that terminal transition while continuing to reject
attachment or reactivation of archived sessions.
