# Session workspaces

Every full Crew session has one Rust-owned workspace record:

```text
{ cwd: absolute normalized path, revision: monotonic integer, updatedAt: timestamp }
```

The workspace is execution context. It resolves relative local-tool paths and
is passed to managed external runtimes when threads or turns start or resume.
It is not a filesystem allowlist or a permission boundary: full-agent tools may
still use absolute paths outside the workspace when their tool contract permits
that operation.

Profiles do not own or default a workspace. A profile may be reused by sessions
in different workspaces. Full-session creation therefore requires an explicit
workspace cwd, and the service has no fallback cwd.

An idle session can switch workspaces without replacing its session, agent,
profile, transcript, or provider lineage record. Call
`POST /v1/admin/control/sessions/{session_id}/workspace` with `cwd` and
`expectedRevision`. Rust rejects stale revisions, active work, relative paths,
and archived sessions. Successful changes are durable before the
`session_workspace_changed` event is published.

Delegated worker `resourceLimits.workdir` is a separate, explicitly scoped
constraint. It may bound worker write and patch tools. That exception must not
be generalized to full sessions, profiles, or ordinary agent tool handling.
