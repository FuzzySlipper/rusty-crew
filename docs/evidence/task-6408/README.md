# Task 6408 Live Provider Evidence

Captured against the SQLite-backed debug service at `http://127.0.0.1:9348`
after rebuilding the native bridge and restarting the service.

## Chat Completions

Profile `task-6408-chat-1785386712` completed one logical turn across two
continuation epochs. The terminal logical-turn projection reports seven
provider operations and six committed tool operations. The provider-reported
input reached 24,598 of 128,000 tokens, triggering
`context_compaction_started` and `context_compaction_completed`. The
replacement retained 11 model-facing items, compacted 18, and estimated 9,561
tokens afterward. No failed event was emitted.

## OpenAI Responses

Profile `task-6408-responses-fresh-1785386957` completed one logical turn
across two continuation epochs with nine provider operations and eight
committed tool operations. It compacted at 10,275 of 250,000 provider-reported
input tokens. The artifact explicitly records
`providerChainAction: rebuild_replay_after_compaction`, proving that hidden
previous-response state was not falsely treated as compacted. No failed event
was emitted.

The compact event and tool-operation details are retained in
[`live-provider-results.json`](live-provider-results.json). Summary text is
represented only by byte length so the evidence remains compact and does not
become a second transcript.

## Rusty View Boundary

Crew returned all 35 Responses events, including the dedicated compaction
events. Selecting the same session in the deployed Rusty View produced
`TypeError: Cannot read properties of undefined (reading findIndex)`, followed
by a blank transcript and `No events yet`. The observed client state is in
[`rusty-view-responses-compaction-debug.png`](rusty-view-responses-compaction-debug.png).

This is a cross-project UI projection defect, not a Crew event-readback
failure. It is recorded on Rusty View task `#6370` in Den message `25841`.
Task `#6371` must rerun the browser portion after that task lands; this packet
does not claim the final View acceptance criterion.
