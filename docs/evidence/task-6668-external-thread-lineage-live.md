# Task 6668 external thread lineage live certification

Date: 2026-08-08
Target: debug SQLite service `http://127.0.0.1:9348`
Runtime: `rv-live-codex-5516`
Profile: `rv-codex-5516-a`

The reusable certificate
`npm run smoke:external-thread-lineage-live -w @rusty-crew/brain-island`
created a disposable full Crew session, completed two native Codex turns, ran
`/new`, restarted `rusty-crew-debug.service`, and completed one independent
turn on the successor.

Observed identities:

- predecessor binding: `external-binding-542e56ee3a53a8434b8dd7c7`
- predecessor Crew session: `external-session-542e56ee3a53a8434b8dd7c7`
- predecessor native thread: `019fe257-1402-7ab1-8a7c-af96bf07395e`
- predecessor turns after restart and successor activity: 2
- successor binding: `external-binding-998c70805161a641290b382b`
- successor Crew session: `external-session-998c70805161a641290b382b`
- successor native thread: `019fe257-47cc-7d70-9a67-4f2b1f2c6742`
- successor turns after its independent activity: 1

Before the successor's first turn, Codex omitted its unmaterialized native
thread from `thread/list`. Crew's binding-backed projection still listed and
read it with zero turns, `nativeMaterialized: false`, and explicit binding,
Crew-session, and lineage fields. It did not copy predecessor turns into the
successor.

The certificate deleted both native threads in `finally`. Readback showed no
active certificate bindings; the durable binding records remain archived as
expected for lifecycle auditability.

During the first restart attempt, task 6692's strict workspace validation
correctly exposed 46 legacy debug configured full sessions without an explicit
workspace. Task 6705 migrated those sessions with the operator-selected `/home`
execution cwd and a rollback backup before this successful run. No profile
workspace or filesystem restriction was introduced.
