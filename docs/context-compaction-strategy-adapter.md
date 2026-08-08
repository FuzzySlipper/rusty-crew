# Context Compaction Strategy Adapter Contract

Status: Crew task 6615 contract, schema version 1.

Rusty Crew owns session lifecycle, the canonical transcript, safe-boundary
selection, strategy execution deadlines, validation, artifact persistence, and
provider-projection application. A downstream product may supply a preservation
strategy; it does not run a competing compactor and must not write Crew session
or transcript state.

The Rust extension point is `BrainContextCompactionStrategy` in
`rusty-crew-brain-runtime`. Crew passes an owned frozen
`BrainContextCompactionStrategyInput` and accepts only a validated
`BrainContextCompactionPreservationDecision`. The provider brains expose
`with_compaction_strategy` at composition time. There is no ambient registry,
profile callback, Den call, or mutable transcript handle.
The host may supply bounded adapter input immediately before a wake with
`set_compaction_domain_context`. The provider brain consumes it as a one-wake
value, freezes it into the strategy input, and never carries it into another
session or later wake implicitly.

## Input

The common input contains:

- a frozen provider-neutral context snapshot with unique source references and
  source-projection lineage; the canonical transcript itself remains outside
  the strategy boundary;
- the selected compaction policy;
- an exclusive Rust-selected safe boundary and any active tool-exchange id;
- optional bounded domain context; and
- the parent artifact id when extending an earlier payload lineage.

For a Rusty Roleplay adapter, `domainContext` version 1 may contain only these
curated inputs:

```ts
interface RoleplayCompactionDomainContextV1 {
  schemaVersion: 1;
  sceneBoundary?: {
    sceneId: string;
    sourceRefs: string[];
    reason: "scene_started" | "scene_ended" | "director_boundary";
  };
  retentionTiers: Array<{
    sourceRef: string;
    tier: "critical" | "scene" | "recent" | "discardable";
    reasonCode: string;
  }>;
  directorsNotes: Array<{
    noteId: string;
    text: string;
    provenanceSourceRefs: string[];
  }>;
  extractionRequests: Array<{
    requestId: string;
    kind: "lore_fact" | "character_fact" | "scene_fact";
    sourceRefs: string[];
  }>;
}
```

All source references must refer to the supplied snapshot. Secrets, raw tool
arguments, provider credentials, unrestricted debug payloads, and mutable
service handles are forbidden.

## Output

The decision contains the strategy id and revision, a user-safe summary, a
complete disjoint partition of compacted and retained source references, a
versioned preservation payload, payload lineage, quality, and warnings.

A Roleplay adapter's `preservationPayload` version 1 may contain:

```ts
interface RoleplayCompactionPreservationPayloadV1 {
  schemaVersion: 1;
  scene?: { sceneId: string; summary: string; sourceRefs: string[] };
  retainedFacts: Array<{
    factId: string;
    kind: "lore" | "character" | "scene";
    text: string;
    sourceRefs: string[];
    confidence: "exact" | "derived" | "uncertain";
  }>;
  directorsNotes: Array<{
    noteId: string;
    text: string;
    sourceRefs: string[];
  }>;
  extractionResults: Array<{
    requestId: string;
    status: "completed" | "partial" | "failed";
    factIds: string[];
    reasonCode?: string;
  }>;
}
```

`quality` is `exact`, `derived`, or `degraded`. A degraded decision must list
warnings with stable reason codes. Strategy failure and timeout are not
decisions: Crew reports a retryable compaction failure/attention, persists the
failed lifecycle result through the task 6613 artifact path, and continues to
select the previous completed artifact during restart hydration.

## Validation and lifecycle

Crew rejects a decision when its strategy identity/revision or payload lineage
does not match the frozen input; source references are missing, duplicated,
overlapping, or unknown; an item at or after the safe boundary is compacted; or
an active tool exchange would be compacted. A rejected or timed-out strategy
has no provider or transcript handles, so its late result cannot alter state.
The same frozen input may be retried.

Validated strategy metadata is stored under
`ContextCompactionArtifact.metadata_json.strategy_payload`. This extends the
durable, idempotent artifact and restart rules certified by Crew task 6613. It
uses the started/completed/failed phases, stable reason codes, user-safe status,
and recoverable-attention semantics certified by Crew task 6614. Raw transcript
and tool telemetry remain authoritative and separately inspectable.

## Rusty Roleplay handoff

Rusty Roleplay should implement this adapter contract against Crew's strategy
input/output types and submit it through Rust composition. It should not add a
second session lifecycle, artifact store, safe-boundary selector, manual
compaction endpoint, or generic rolling-summary implementation. Any need to
change Crew's generic accounting, admission, persistence, or provider contracts
must be demonstrated as a concrete incompatibility rather than assumed by the
downstream adapter.
