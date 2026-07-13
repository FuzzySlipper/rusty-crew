# Roleplay Mechanic Proposals

Roleplay mechanic agents diagnose roleplay sessions and may create durable
change proposals. They never apply changes directly. Rust owns target capture,
review state, optimistic conflict detection, application, audit history, and
SQLite/Postgres persistence.

## Lifecycle

1. `propose_roleplay_change` creates a `proposed` record and captures the
   current target value and revision. The target is unchanged.
2. An operator approves or rejects the proposal through the admin API.
3. Only an `approved` proposal can be applied. Apply rechecks the captured
   target and records either `applied` or an audited conflict.
4. Repeating the same create, decision, or successful apply is idempotent.

Supported kinds are `narrator_config`, `exemplar`, `lore_add`, `lore_edit`,
`lore_tags`, `layer_retrieval_config`, and `provider_failure_pattern`.

## Mechanic Tool Input

The mechanic receives a single-string Markdown tool instead of a multi-field
JSON form. YAML front matter carries routing and review context; the body is
the proposed value. Structured proposal bodies use YAML. Exemplar bodies are
plain text.

```markdown
---
roleplay_session_id: rp-session-1
change_kind: exemplar
rationale: Recent turns drifted away from the established concrete voice.
evidence:
  - recall-trace-12
  - wake-44
---
Rain counted a patient rhythm against the observatory glass.
```

Kinds that modify a lore record or layer also require `target_id`. The tool
only creates the proposal; it has no approve, reject, or apply operation.
Narrator profiles do not receive mechanic proposal tools.

## Admin API

All routes use the standard admin envelope.

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/v1/admin/roleplay/mechanic-proposals` | List/filter proposals |
| `POST` | `/v1/admin/roleplay/mechanic-proposals` | Create without applying |
| `GET` | `/v1/admin/roleplay/mechanic-proposals/{proposal_id}` | Read one proposal |
| `GET` | `/v1/admin/roleplay/mechanic-proposals/{proposal_id}/history` | Read audit history |
| `POST` | `/v1/admin/roleplay/mechanic-proposals/{proposal_id}/approve` | Approve at an expected proposal revision |
| `POST` | `/v1/admin/roleplay/mechanic-proposals/{proposal_id}/reject` | Reject at an expected proposal revision |
| `POST` | `/v1/admin/roleplay/mechanic-proposals/{proposal_id}/apply` | Apply an approved proposal |

List filters accept snake-case or camel-case mechanic session, roleplay
session, and profile IDs, plus `status`, `kind`, `limit`, and `offset`.
Approval and rejection require `reviewerId` and `expectedRevision`. Apply
requires `actorId`.

Profile-backed changes are materialized into the service profile config after
Rust commits the applied proposal, then the affected runtime is refreshed.
Lore and retrieval changes are already fully Rust-owned and do not require a
TypeScript-side mutation path.

## Certification

Focused deterministic coverage lives in the core-engine proposal tests and the
mechanic profile/diagnostic smokes. The live provider gate is:

```bash
npm run smoke:roleplay-mechanic-proposals-live -w @rusty-crew/brain-island
```

It is pinned to the SQLite-backed debug service on port `9348`, uses a real
mechanic LLM tool call, restarts the service, and proves approval/apply
separation, idempotent apply, durable history, and inert rejection.
