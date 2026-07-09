# GitHub Check Gates

Rusty Crew runs its ordinary offline gate in GitHub Actions with two required
GitHub check runs:

- `Verify Offline`
- `Verify Postgres Backend`

The workflow is `.github/workflows/offline-ci.yml` and its workflow name is
`Offline CI`, but Den Review gates match GitHub **job/check-run names**, not the
workflow name or workflow filename. Do not register `Offline CI`, `build`,
`test`, or `lint` as required checks unless the workflow jobs are renamed to
those exact check-run names.

For Den Review GitHub check gates, use:

```json
{
  "project_id": "rusty-crew",
  "task_id": "<den-task-id>",
  "repository": "FuzzySlipper/rusty-crew",
  "commit_sha": "<full-40-character-sha>",
  "ref": "main",
  "required_checks": ["Verify Offline", "Verify Postgres Backend"],
  "requested_by": "<agent-name>"
}
```

Agents should register the exact pushed commit SHA after a task commit is
pushed. The Den service records pass, fail, timeout, or superseded evidence on
the task thread; GitHub Actions remains the runner.
