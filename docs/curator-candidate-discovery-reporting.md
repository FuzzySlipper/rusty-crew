# Curator Candidate Discovery And Reporting

Rusty Crew curator scans start as deterministic candidate discovery and report
generation. This keeps curator work useful before durable mutation persistence
lands, while preserving the boundary that discovery must not mutate profile,
skill, or memory state.

## Implemented Surface

`discoverCuratorCandidates` accepts a scoped review input and returns a
`CuratorCandidateBatch`. The batch includes:

- a stable batch id and report id;
- the scope type and scope id used for discovery;
- proposed curator candidates;
- skipped reasons, including candidate limit truncation.

`renderCuratorCandidateReport` converts the batch into a compact Markdown
operator report with severity and kind counts plus candidate references.

## Candidate Kinds

The first implementation intentionally favors safe, explainable static checks:

- `skill_patch`: missing summaries, oversized skill bodies, and duplicate skill
  titles;
- `skill_create`: profile-expected skill slugs that were not loaded;
- `dense_memory_prune`: oversized dense profile memory or temporary-progress
  language;
- `dense_memory_merge`: duplicate dense profile memory keys;
- `diagnostics_only`: suspicious but non-mutating findings, such as TODO-like
  project progress inside skill guidance.

The type surface reserves `skill_archive` and `sidecar_write`, but discovery
does not emit them yet. Those should wait until snapshot-backed mutation and
rollback handling exists.

## Mutation Boundary

Discovery returns only proposed candidates. It must not:

- edit skills;
- edit profile configs;
- edit dense memory;
- write sidecar state;
- approve or apply curator work.

Application is still owned by the governance executor behind
`curator_execute`, and the state rules in
`curator-state-snapshots-mutation-boundaries.md` remain the source of truth for
approval, snapshot, rollback, and stale-source checks.

## Determinism

Candidate ids are derived from the batch id, candidate kind, target ref,
summary, and source refs. Re-running discovery over the same input produces the
same candidate ids. This matters because future approval flows should not
resurrect stale proposals or silently apply a candidate to changed source
content.

## Current Limits

The implementation does not call an LLM, infer semantic replacements, or write
patches. It is a discovery/reporting scaffold for background curator loops and
admin/operator review. Future work can add LLM-backed proposal generation after
the snapshot and approval path is durable.
