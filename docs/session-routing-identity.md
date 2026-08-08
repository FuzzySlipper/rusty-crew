# Session routing identity

Profiles are reusable brain, prompt, model, and tool configuration. A profile's
stable `agent_id` identifies that reusable agent configuration; it is not a
unique live execution address.

Every live Crew session has its own `session_id` and persisted agent-instance
record. Multiple active full sessions may share the same profile and stable
agent ID. Their transcripts, workspaces, logical turns, queues, cancellation,
and lifecycle state remain keyed by session ID.

## Address resolution

- A switchboard `@route` resolves to one revisioned session/binding target.
- A raw agent ID remains compatible while exactly one matching session is
  active.
- A raw agent ID with multiple active sessions fails with
  `agent_session_ambiguous` and a deterministic `candidate_session_ids` list.
  Callers must select or create an exact `@route`; Crew never picks a sibling.
- Archiving one sibling makes a formerly ambiguous raw address unambiguous when
  exactly one active sibling remains. It does not retarget an existing route.

Routed bus messages carry optional `from_session_id` and `to_session_id` facts.
New Crew delivery paths populate the exact identities before publication, so
session event subscriptions and frozen body snapshots cannot consume a
sibling's message. Events persisted before these fields existed use agent-only
fallback only when the active agent-to-session mapping is unambiguous.

## Workspace and permissions

Each sibling owns the ordinary session workspace described in
`docs/session-workspaces.md`. Sharing a profile does not share or constrain a
workspace. Session routing identity introduces no filesystem restriction or
harness permission boundary; delegated/subagent confinement remains a separate
special-case contract.
