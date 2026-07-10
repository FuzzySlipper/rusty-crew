# Roleplay Live Certification

Task: #5390
Date: 2026-07-10

## Substrates

The same live-provider certification ran against both installed services:

- SQLite debug service: `http://127.0.0.1:9348`, schema 33,
  `/home/system/rusty-crew-debug`;
- PostgreSQL live service: `http://127.0.0.1:9347`, schema 18,
  `/home/system/rusty-crew`;
- provider: `deepseek-flash` through local den-router;
- brain: Rust pi-agent loop with the `roleplay_narrator` strategy;
- local tool profile: `roleplay_lore`.

Both services reported the compiled `roleplay` module. The storage query catalog
reported eleven read-only queries after the run, including:

- `roleplay.characters`;
- `roleplay.personas`;
- `roleplay.sessions`;
- `roleplay.imports`.

A source sweep found no `simple_kv` calls in the roleplay routes, roleplay Rust
crates, or roleplay persistence repositories. Roleplay truth is held by typed
records and module-owned tables rather than generic KV fallback.

## Live Sequence

`smoke-roleplay-persistence-live.ts` provides explicit `prepare`, `verify`, and
`cleanup` phases. Each backend ran this sequence:

1. Create a disposable profile and separate DB-owned roleplay chat session.
2. Create character, player persona, and narrator configuration records.
3. Create auto-capture and durable lore layers and bind them to the chat.
4. Capture a fact, promote it to the durable layer, and verify supersession.
5. Run a real narrator turn and require scene lookup, lore recall,
   explore/compose phases, and clean narrative output.
6. Generate one model-backed alternate without normal-chat event leakage.
7. Create a manual alternate, select it, then reselect the generated alternate.
8. Restart the service.
9. Verify metadata, layer bindings, promoted lore, variants, selected variant,
   branch continuity, and suppressed generation events.
10. Run another real narrator turn after restart.
11. Hard-delete the disposable profile and its owned records.

SQLite evidence:

- prepare narrative: 934 characters;
- generated variant:
  `rp-cert-sqlite-1783669873409-generated-alt`;
- post-restart session state: `idle`;
- post-restart follow-up narrative: 1,308 characters;
- cleanup completed.

PostgreSQL evidence:

- prepare narrative: 1,174 characters;
- generated variant:
  `rp-cert-postgres-1783670629872-generated-alt`;
- post-restart session state: `idle`;
- post-restart follow-up narrative: 940 characters;
- cleanup completed.

## Defects Found

The real sequence found three gaps that deterministic route tests had not
exposed.

### Model-Facing Lore Capture

The model could provide valid top-level capture fields while omitting the
storage-specific `evidenceRefs` list or repeating canonical fields inside
`content`. Rust correctly rejected those incomplete persistence shapes, but the
tool adapter made the model reproduce storage ceremony.

The adapter now derives canonical content fields from top-level tool arguments
and creates a tool-call provenance reference when evidence is omitted. Rust
storage validation remains strict.

The existing four-turn narrator quality smoke then passed with live lore recall,
fact capture, continuity, clean output, and `exploring -> composing -> idle`
phase activity. Its cleanup now hard-deletes disposable profiles on success and
failure.

### Durable Assistant Message Slots

Normal chat wakes streamed assistant text but persisted only the user's message
slot. The alternatives API therefore saw the durable terminal message as a user
message and correctly rejected alternate generation.

Completed normal chat wakes now persist the assistant response as one atomic
Rust slot/primary-variant/branch-head update. Alternate-generation wakes still
set `appendChatEvents: false` and persist only through the atomic alternate
operation, so they do not append a duplicate normal assistant turn.

### DB-Owned Roleplay Session Restart

Orderly shutdown archives active runtime sessions. Startup reactivated sessions
listed in file runtime config, but dynamically created roleplay sessions are
DB-owned and intentionally absent from `service.json`.

Rust initialization now reactivates only roleplay sessions whose typed metadata
has `archived: false`, using each session's persisted immutable config. Explicitly
archived roleplay sessions remain archived. A Rust restart test covers both
states, and both installed backends passed real restart verification.

## Rendered Rusty View Proof

Live scenario:

- test: `baseline multi-turn real conversation`;
- backend: `http://127.0.0.1:9348`;
- profile: `rp-cert-sqlite-1783669873409-profile`;
- provider/brain: `deepseek-flash`, Rust pi-agent roleplay narrator;
- broker run index:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260710T080625.387602640Z-3432465/run-index.json`;
- live artifacts:
  `/tmp/rusty-view/playwright-output/3433193/live-baseline-multiturn.li-a186b-ion-live-agent-conversation-chromium/live-artifacts`.

Inspected screenshots:

- `04-assistant-complete.png`;
- `06-assistant-complete.png`.

Rendered behavior observed:

- both user turns and both completed assistant turns were visible;
- `get_scene_state` and `recall_lore` activity remained attached to each turn;
- reasoning appeared behind a collapsed disclosure;
- prose was clean and readable;
- the connection indicator was connected;
- the second turn remained visible beneath the first without duplication.

`visible-transcript.txt`, `debug-snapshot.json`, `evidence-packet.json`, and
`trace.zip` were present. `page-errors.json` was empty. Console output contained
only Vite connection messages and Angular's development-mode notice.

## Rusty Roleplay Boundary

The dedicated Rusty Roleplay frontend is still a mock-data shell, so it cannot
honestly certify the live Crew roleplay transport yet. Its existing package
boundary Playwright proof was run twice; both attempts timed out because the
visible `Sister A` profile button was repeatedly detached while Angular
re-rendered.

This is tracked downstream as `rusty-roleplay#5550`. That task covers stable
profile identity plus a broker-managed real Crew scenario once the frontend
transport is wired. This downstream UI limitation does not weaken the Crew API,
storage, restart, narrator, alternate, or Rusty View evidence above.

## Supporting Checks

- `cargo fmt --all --check`
- `cargo test -p rusty-crew-core-engine restart_reactivates_only_roleplay_sessions_with_active_metadata --lib`
- `cargo check -p rusty-crew-core-persistence --features postgres`
- `npm run typecheck`
- `npm run smoke -- lore-memory-tool`
- `npm run smoke -- storage-query-catalog`
- `npm run smoke -- rusty-view-chat-read-api`
- `npm run smoke -- roleplay-quality-spike-live`
- `npm run smoke -- roleplay-persistence-live` in prepare/verify/cleanup modes
  on ports 9348 and 9347

Both disposable certification profiles and sixteen historical
`quality-spike-*` debug profiles were hard-deleted after evidence collection.
