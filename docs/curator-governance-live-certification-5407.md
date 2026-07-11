# Curator Governance Live Certification

Task: `5407`

Date: 2026-07-10

## Substrate

- Service: `rusty-crew-debug.service`
- Base URL: `http://127.0.0.1:9348`
- Service root: `/home/system/rusty-crew-debug`
- Storage: SQLite in the isolated debug-service data directory
- Candidate: `curator:cert-5407:create`
- Mutation: `curator-mutation:curator:cert-5407:create:80ec410040`

The certification data is disposable debug-service data. The generated skill
was removed by the tested rollback.

## Live Flow

The running service completed these guarded operations in order:

1. Persisted a typed `skill_create` candidate and `candidate_discovered`
   receipt.
2. Previewed the candidate without writing the skill.
3. Approved the exact candidate fingerprint through the admin control path.
4. Applied the candidate and created `curator-cert-5407.md` under the debug
   skills root.
5. Rolled back the mutation and confirmed the created skill was removed.
6. Restarted `rusty-crew-debug.service`.
7. Read the candidate, rolled-back mutation, and sequenced receipts through the
   admin API after restart.

The final accepted receipt was sequence `9`,
`curator-receipt:rollback_completed:a7e96f158c4464f6f0d8`. Curator status after
restart restored that receipt and sequence. The mutation remained revision `2`
with status `rolled_back`.

## Readback And Path Safety

The following read-only routes returned bounded envelopes with `items`,
`total`, `limit`, `offset`, and `next_offset`:

- `GET /v1/admin/curator/candidates`
- `GET /v1/admin/curator/mutations`
- `GET /v1/admin/curator/audit-receipts`

The persisted/read-back mutation used `curator-cert-5407.md` for both
`changedPaths` and `management.skillPath`. Its snapshot manifest used a
snapshot-root-relative directory reference. No deployment-absolute skill or
snapshot path crossed the API boundary.

Candidate and mutation IDs contain colons. The live flow used percent-encoded
path components and confirmed that the admin route decodes each identifier
before dispatch.

## Observation Evidence

Every governance transition persisted a sequenced neutral activity receipt;
the audit readback showed discovery, preview, approval, apply, and rollback in
order. `smoke:curator-observation` runs the same receipt projection through the
real `AgentActivityObservationProducer`, verifies the published event shape,
then forces a sink failure and verifies bounded degraded behavior. The isolated
debug service has no Den gateway credentials, so this certification does not
claim a live Den write.

## Backend Coverage

The SQLite repository tests cover transactional write, lifecycle transition,
rollback, restart hydration, exact paging, and concurrency behavior. The
ignored live PostgreSQL curator repository test is run explicitly with the
local Rusty Crew PostgreSQL URL. Together with this debug-service flow, those
tests certify both storage implementations without treating a mock as the live
service proof.
