# Task 6705 legacy session workspace rollout

Date: 2026-08-08

The first post-6692 debug restart failed before engine hydration with
`sessions[0].workspaceCwd: full sessions require an explicit workspaceCwd (91 additional diagnostics)`.
The debug `service.json` contained 46 legacy full sessions and production
contained 4; none had a session-owned workspace.

The new operator command was dry-run first and then invoked with an explicit
absolute migration cwd of `/home`, preserving the deployments' prior execution
context while making it durable per session:

```bash
npm run config:migrate-session-workspaces -w @rusty-crew/service-host -- \
  --config <service.json> --workspace-cwd /home --write
```

Rollout results:

- debug: 46 sessions migrated; rollback backup
  `/home/system/rusty-crew-debug/config/service.json.pre-workspace-task-6705`
- production: 4 sessions migrated without restarting production; rollback
  backup `/home/system/rusty-crew/config/service.json.pre-workspace-task-6705`
- post-write readback: zero missing full-session workspaces in either config
- debug restarted successfully and passed the task 6668 multi-turn lineage
  certificate
- production remained active throughout and was not restarted

The migration requires the operator value, never reads profiles, never adds
roots/exclusions/allowed paths, and leaves delegated `resourceLimits.workdir`
unchanged.

## Review correction: mixed legacy records

Round 4167 found a mixed legacy shape with both an explicit `workspaceCwd` and
the retired full-session `resourceLimits.workdir`. Workspace insertion and
retired-field cleanup are now independent: an existing explicit workspace is
preserved, a missing workspace receives only the operator-supplied absolute
value, and every full-session workdir is removed. Delegated workdirs remain
untouched. Focused tests cover existing/missing workspaces with and without the
retired field.
