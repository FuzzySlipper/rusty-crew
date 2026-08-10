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

## Initial Roleplay narrator quality probe

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

## Passing narrator compaction and restart proof

Task 6754 was then implemented. The narrator wrapper now preserves the compose
phase provider state across logical turns, and phase construction passes current
scene and recalled-lore evidence into the Roleplay strategy. Rust resolves the
evidence to narrative source references before the safe boundary and validates
the resulting context before producing an artifact.

The corrected quality probe ran on debug service revision
`8a19c8a893b5182e9383d1ca53f3f4572ca5caee` with the real
`deepseek-flash` provider. It established eight multi-phase narrator turns,
completed manual scene-aware compaction artifact
`manual_quality_quality_spike_1786355899816_1`, paused, restarted
`rusty-crew-debug.service`, and submitted the continuation after hydration.
The post-restart response:

- recalled the silver locket;
- recalled the serpent-and-rose crest / Northern Court connection;
- remained anchored to the prior orchard scene; and
- completed without technical artifacts in the narrative.

The artifact used strategy `roleplay_scene_aware_compaction` revision
`roleplay_scene_aware_v1`. Its preservation payload contained current scene and
recalled lore director notes with validated `chat-message-*` provenance. The
test profile and its session were deleted after the proof. The bounded provider
fixture was archived separately after certification.
