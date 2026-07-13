# Roleplay Mechanic Diagnostics

Mechanic profiles use the built-in `roleplay_mechanic` local tool profile. The
profile is read-only except for the proposal operations introduced by the
separate mechanic proposal lifecycle. Narrator profiles do not receive these
diagnostic tools.

## Read Tools

- `inspect_roleplay_transcript(sessionId, limit?)` returns bounded selected
  user/assistant message variants with speaker, slot, variant, source, and time
  provenance. Tool-role rows and empty messages are excluded.
- `inspect_roleplay_scene(sessionId)` returns typed roleplay-session ownership,
  the current scene-state record, the latest narrator diagnostic snapshot,
  active chat layers, and the narrator profile/provider/style configuration.
- `inspect_lore_retrieval(sessionId, limit?)` returns bounded persisted recall
  traces, including candidate scores, token estimates, inclusion decisions,
  budgets, and layer configuration snapshots.
- `search_lore` and `list_lore_layers` are shared read-only lore operations made
  available to the mechanic tool profile. Their write-side companions remain
  absent.

Every diagnostic operation requires a typed roleplay session record. A missing
session fails with `roleplay_session_not_found`; a missing scene-state record,
narrator diagnostic snapshot, recall trace, or profile is reported explicitly
as `status: "missing"` instead of being represented as an empty successful
record.

## Durable Sources

The narrator FSM's latest scene brief and relevant lore record IDs are stored as
the `narratorDiagnostic` projection on typed roleplay session metadata. The
projection is written after a successful narrator turn and survives service
restart on both SQLite and PostgreSQL.

Lore recall traces persist an `entry_decisions` array. Each item records the
record and layer IDs, score, token estimate, constant status, inclusion status,
and one of these reasons:

- `included`
- `excluded_subject`
- `constant_reserve_exceeded`
- `token_budget_exceeded`

The existing aggregate counts and configuration snapshot remain available.
Mechanic tools read these repositories through the native bridge; they do not
scrape observation logs or maintain TypeScript shadow state.

## Verification

Focused checks:

```bash
npm run smoke:roleplay-mechanic-diagnostics -w @rusty-crew/brain-island
npm run smoke:roleplay-narrator-brain -w @rusty-crew/brain-island
cargo test -p rusty-crew-core-persistence roleplay_lore --features postgres-backend
```

The debug-only live certification creates disposable narrator and mechanic
profiles, writes real lore, runs a narrator turn, restarts the service, and
requires a live mechanic model to call all three diagnostic tools:

```bash
npm run smoke:roleplay-mechanic-diagnostics-live -w @rusty-crew/brain-island
```
