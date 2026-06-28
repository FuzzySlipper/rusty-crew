# Capture Producer Prompt And Typed Output Schema

Status: implementation contract for capture decision producer Phase 2

Related tasks:

- #3590 Implement Capture Decision Producer Digest And Proposal Pipeline
- #3597 Design Capture Producer Prompt And Typed Output Schema

## Role

The capture producer is a maintenance analysis call. It is not a full brain wake,
does not receive tools, does not use provider wire state, and does not deliver
messages. It reads bounded session activity digests and proposes durable memory
candidate records for the existing memory proposal/curator path.

## Initial Target Policy

Phase 2 validates only `profile_dense` proposals.

Future targets are intentionally represented in the typed schema but should stay
disabled until their policy/UI paths are ready:

- `session_memory`
- `roleplay_lore`

No auto-apply happens in this phase. Every proposal is routed to pending review.

## Prompt Sections

The prompt should contain these sections in order:

1. Task: identify durable facts worth preserving from recent session activity.
2. Durability rules:
   - Durable: stable environment facts, user/project conventions, corrected
     assumptions, failed approaches with durable rationale, repeated workflow
     constraints.
   - Ephemeral: one-time build errors, current service status, temporary
     incidents, task-local commands, transient logs.
   - Never capture: secrets, raw private content without durable meaning,
     operational status such as "server is up now".
3. Allowed target spaces: initially `profile_dense` only.
4. Current dense profile memory and loaded skills for dedupe.
5. Recent `SessionActivityDigest` summaries.
6. JSON output rules and maximum proposal count.

## Output Envelope

The model returns JSON:

```json
{
  "proposals": [
    {
      "summary": "Remember where the Den Core database lives.",
      "space_id": "profile_dense",
      "operation": "add",
      "scope": { "scope_type": "profile", "scope_id": "rusty-crew-runner" },
      "shape": { "shape_id": "profile_dense_item", "version": 1 },
      "content": {
        "key": "den_core_database_location",
        "content": "The Den Core database lives on den-srv, not on agent workstations.",
        "metadata_json": {
          "capture_summary": "User corrected database host topology."
        }
      },
      "evidence_refs": [
        {
          "eventType": "user_correction",
          "wakeId": "wake-alpha",
          "summary": "User corrected a stale database host assumption."
        }
      ],
      "confidence": 0.86,
      "durability_rationale": "Infrastructure topology applies across future sessions.",
      "governance_policy": "curator_route",
      "dedupe_key": "profile_dense:add:profile:rusty-crew-runner:den_core_database_location"
    }
  ],
  "skippedReasons": []
}
```

The TypeScript normalized proposal type is `TypedCaptureMemoryProposal`.
Conversion to `MemoryProposalEnvelope` must go through
`captureProposalToMemoryProposal`.

## Validation Rules

- `proposals` must be an array.
- Unknown spaces are rejected in Phase 2 unless explicitly enabled by policy.
- `profile_dense` proposal content must include a non-empty `key`.
- `add`, `replace`, and `merge` proposals must include useful content.
- Confidence is clamped or rejected outside `0..1`; implementation should
  prefer rejection for malformed provider output.
- Proposals without evidence are rejected.
- Provider output beyond the configured maximum proposal count is truncated after
  validation diagnostics are recorded.

## Skip And Failure Reasons

Recommended reason codes:

- `capture_provider_alias_missing`
- `capture_provider_unavailable`
- `capture_provider_invalid_json`
- `capture_provider_invalid_proposals`
- `capture_no_session_activity_digests`
- `capture_no_supported_proposals`
- `capture_provider_timeout`

These should surface through background review `skippedReasons` and service
diagnostics where possible.
