# Historical docs

These documents record architecture state at a point in time and have been
superseded by landed work. They are kept as audit/execution records, not as
current architecture truth. For current policy use `docs/README.md`, the
`docs/adr/` trail, the repo root `README.md` / `AGENTS.md`, and the Den
document `rusty-crew-unified-architecture`.

## Moved here

- `architecture-review-2026-07-01.md` — read-only review of the repo at commit
  `de2a31a`. Its own status note (2026-07-03) declares it a historical review
  input; the findings it raised were since addressed or converted into ADRs
  (Rust brain modules → ADR 0021, Crew-owned service storage → ADR 0022,
  service-host extraction, storage scope governance, live deliverable
  certification, bridge streaming responsiveness).

- `architecture-remediation-plan.md` — the implementation plan paired with the
  review above. Its own status note (2026-07-03) declares it an execution
  record; the remediation series landed (ADRs 0021/0022, `ts/packages/
  service-host` extracted as composition root). Do not treat early-phase
  wording as current policy.

- `architecture-review-2026-07-05.md` — five-track durability review of the
  repo at commit `4dd0199`, taken immediately before the remediation wave. Of
  its seven ranked risks, CI, ops hardening, toolchain pinning, commit
  history, the persistence/bridge god-file splits, and Postgres
  migrations/pooling/CI-tested conformance have since landed; the open
  remainder (composition-layer ownership, smoke relocation, bridge coverage,
  test pyramid) is re-ranked in `docs/architecture-review-2026-07-06.md`,
  which supersedes it as the current snapshot.

- `postgres-full-service-gap-audit.md` — audit of pre-cutover PostgreSQL state
  for task 3502. Its own status line declares it historical and instructs
  readers not to use its "Current Live Deployment" section as current
  operational truth. The cutover has since landed: the live service at
  `/home/system/rusty-crew` (port 9347) runs PostgreSQL; see
  `docs/local-service-topology.md` and `docs/system-rusty-crew-install-layout.md`.
