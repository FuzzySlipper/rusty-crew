# Task 6618 Roleplay compaction live evidence

Date: 2026-08-10

## Generic lifecycle proof

The debug Rusty Crew service on port 9348 ran the generic context-compaction
certification with a real `deepseek-flash` provider through den-router. The
service source revision was
`abe42345520079e0bf0558179d0fbc35d1ad988d`; the evidence packet is:

`/home/system/rusty-crew-debug/evidence/task-6618/msmzyp5k/live-results.json`

The run completed 20 turns and 20 tool calls, triggered 19 compactions, retained
the exact continuity marker `RP_CONTINUITY_6618_SILVER_LOCKET_PROMISE` across
restart, preserved durable transcript counts across hydration, and exercised an
explicit failed-compaction path without replacing the previous provider state.
Temporary test profiles and providers were archived after the run.

## Roleplay narrator quality probe

The Roleplay narrator quality probe ran against the same debug service after it
was updated to exact source revision
`3cd9731c81a29dc1029f0def74d8a5420fc9bc70`.

Before compaction pressure, the live narrator:

- called `recall_lore` and queried scene state;
- produced a clean narrative response from that evidence; and
- called `capture_lore_fact` for the established locket and crest continuity.

After applying `roleplay_scene_aware_compaction` with a deliberately low
threshold, Crew emitted `context_compaction_started` followed by
`context_compaction_failed`. The reported reason was:

> context pressure exceeded the configured threshold, but no completed
> historical exchange can be compacted without touching the frozen request or
> pending tool context

This is a valid safe-boundary failure. The narrator creates a new provider host
for every explore and compose phase over the current frozen wake, so the phase
that encounters pressure has no completed phase-local history eligible for
compaction. The probe did not show post-compaction Roleplay continuity and must
not be represented as a passing quality certification.

Rusty Roleplay task 6754 tracks the required durable continuity projection
across phase-isolated narrator wakes. The follow-up preserves Crew ownership of
canonical transcript, lifecycle, artifacts, and safe-boundary selection. It
explicitly excludes write-time deletion or mutation of canonical transcript and
tool telemetry.
