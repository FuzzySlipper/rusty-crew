# Stale Review Discovery

Use the Rusty Crew review CLI to find Den tasks that appear to have missed the
normal reviewer dispatch after their exact GitHub gate passed:

```bash
rusty-crew-review stale \
  --service-url http://127.0.0.1:9347 \
  --deployment-role production \
  --stale-ms 300000
```

The default scan covers every current normal Den project. Repeat `--project`
to limit it:

```bash
rusty-crew-review stale \
  --service-url http://127.0.0.1:9347 \
  --deployment-role production \
  --project den-services \
  --project rusty-crew \
  --stale-ms 600000
```

Default output is intentionally only one `project#task` handle per line. No
matches is a successful command with no output. Add `--json` for a stable array
of `{ "projectId", "taskId" }` objects.

A task is listed only when its Den status is `review`, its latest round and
latest gate refer to the same exact commit, that gate is `passed`, the round has
no verdict, the latest relevant task/round/gate activity is at least
`--stale-ms` old, and Rusty Crew has no managed submission for that exact
commit. Pending, failed, recent, checkless/direct, already reviewed, and
actively managed reviews are omitted.

The command is read-only. It does not prompt `@reviewer`, create a review,
register a gate, or repair a submission. A service or project-scan error exits
nonzero so a partial list is never presented as complete.
