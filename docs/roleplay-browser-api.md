# Roleplay Browser API

Status: implemented browser-facing service contract.

All routes use the normal service JSON envelope:

```json
{ "ok": true, "data": {}, "meta": { "request_id": "...", "schema_version": 1 } }
```

Errors use `ok: false` with `error.code`, `error.reason_code`, `error.message`,
and `error.retryable`. Routes are bearer-token protected like the chat/admin
APIs and participate in browser CORS preflight.

## Lore Layers

Canonical routes:

- `GET /v1/admin/roleplay/lore/layers?profile_id=<profileId>`
- `POST /v1/admin/roleplay/lore/layers`
- `GET /v1/admin/roleplay/lore/layers/{layerId}`
- `PATCH /v1/admin/roleplay/lore/layers/{layerId}`
- `DELETE /v1/admin/roleplay/lore/layers/{layerId}`
- `GET /v1/admin/roleplay/lore/layers/{layerId}/entries`
- `GET /v1/admin/roleplay/lore/layers/{layerId}/entries/{entryId}`
- `PATCH /v1/admin/roleplay/lore/layers/{layerId}/entries/{entryId}`
- `GET /v1/admin/roleplay/lore/entries/search`
- `POST /v1/admin/roleplay/lore/entries`
- `GET /v1/admin/roleplay/lore/entries/{entryId}`
- `PATCH /v1/admin/roleplay/lore/entries/{entryId}`
- `POST /v1/admin/roleplay/lore/entries/{entryId}/promote`

Compatibility route for roleplay clients:

- `GET /v1/profile/{profileId}/layers`
- `POST /v1/profile/{profileId}/layers`

Create accepts camelCase or snake_case:

```json
{
  "layerId": "world-main",
  "profileId": "narrator-profile",
  "name": "World",
  "description": "Shared world lore",
  "purpose": "world",
  "writePolicy": "manual"
}
```

Layer list responses include `layers`, `entryCounts`, and `total`; each layer is
augmented with `entry_count` and `entryCount`.

## Lore Entry Search

`GET /v1/admin/roleplay/lore/entries/search` searches stored lore entries
without invoking an agent tool.

Query parameters:

- `q` or `query`: text search across title/body.
- `profile_id`: filters to entries linked to lore layers owned by a profile.
- `chat_id`: filters to entries linked to active lore layers for a chat.
- `layer_id` / `layer_ids`: explicit layer filter. May be repeated or
  comma-separated.
- `world_id`, `entity_id`, `canon_status`, `visibility`, `shape_id`: direct
  lore record filters.
- `include_superseded`, `include_tombstoned`: boolean flags.
- `limit`, `offset`: bounded paging controls.

Response data:

`hasMore` is authoritative for paging. `total` is exact when `totalExact` is
true. For unscoped searches, `total` is a known lower bound based on bounded
over-fetching so the route can avoid materializing every matching lore record
just to count.

```json
{
  "query": {
    "text": "clockmaker",
    "profileId": "narrator-profile",
    "chatId": "session-id",
    "layerIds": ["world-main"]
  },
  "entries": [{ "record_id": "entry-id", "title": "Clockmaker Song" }],
  "layers": [{ "layer_id": "world-main", "name": "World" }],
  "layerContext": {
    "source": "profile",
    "profileId": "narrator-profile",
    "chatId": null,
    "layerIds": ["world-main"],
    "activeLayerIds": ["world-main"]
  },
  "total": 1,
  "totalExact": true,
  "limit": 50,
  "offset": 0,
  "hasMore": false
}
```

## Lore Entry Detail And Editing

`GET /v1/admin/roleplay/lore/entries/{entryId}` returns one browser-safe lore
entry without invoking an agent tool. Optional `profile_id`, `chat_id`, or
`layer_id` / `layer_ids` query parameters add layer-membership context to the
response.

```json
{
  "entry": {
    "record_id": "entry-id",
    "revision": 2,
    "title": "Clockmaker Song",
    "body": "...",
    "primary_keys": ["clockmaker"],
    "secondary_keys": ["silver leaves"],
    "enabled": true,
    "scan_depth": 4,
    "insertion_position": "lore_block",
    "insertion_order": 0,
    "probability": 1,
    "retrieval_role": "system",
    "lore_controls": {
      "primary_keys": ["clockmaker"],
      "secondary_keys": ["silver leaves"],
      "enabled": true,
      "constant": false,
      "scan_depth": 4,
      "insertion_position": "lore_block",
      "insertion_order": 0,
      "probability": 1,
      "retrieval_role": "system"
    },
    "lore_control_support": {
      "primary_keys": "stored_only",
      "secondary_keys": "stored_only",
      "enabled": "stored_only",
      "scan_depth": "stored_only",
      "insertion_position": "stored_only",
      "probability": "stored_only",
      "retrieval_role": "stored_only",
      "constant": "layer_entry_recall",
      "insertion_order": "layer_entry_priority_recall"
    }
  },
  "provenance": [
    {
      "event_id": "entry-id:created",
      "record_id": "entry-id",
      "source": "ui"
    }
  ],
  "supersession": {
    "supersedesRecordId": null,
    "supersededByRecordId": null,
    "supersedes": null,
    "supersededBy": null
  },
  "layerEntries": [
    {
      "layer_id": "world-main",
      "record_id": "entry-id",
      "constant": false,
      "insertion_order": 0
    }
  ],
  "layers": [{ "layer_id": "world-main", "name": "World" }],
  "layerContext": {
    "source": "explicit",
    "profileId": null,
    "chatId": null,
    "layerIds": ["world-main"],
    "activeLayerIds": ["world-main"]
  }
}
```

### Lore Trigger And Insertion Controls

Lore entry create/update accepts stable trigger/insertion controls either as
top-level fields, under `controls`, or under `lore_controls`. The canonical
stored shape is `content.lore_controls`; read/search/detail responses also
mirror the fields onto the browser-safe `entry` object for simple UI binding.

Supported request fields:

- `primary_keys`: string array or comma-separated string.
- `secondary_keys`: string array or comma-separated string.
- `enabled`: boolean, default `true`.
- `constant`: boolean, default `false`. On entry create, this also seeds the
  layer-entry `is_constant` link unless `is_constant` / `isConstant` is supplied.
- `scan_depth`: integer `0..200`, default `4`.
- `insertion_position`: one of `before_history`, `after_history`,
  `before_author_note`, `after_author_note`, `system`, or `lore_block`.
- `insertion_order`: integer `-1000000..1000000`, default `0`. On entry create,
  this also seeds the layer-entry `priority` link unless `priority` is supplied.
- `probability`: number `0..1`, default `1`.
- `retrieval_role`: one of `system`, `user`, `assistant`, or `narrator`.

Current recall support is explicit in `lore_control_support`: layer-entry
`constant` participates in `recall_lore` as always-on lore, and
`insertion_order` participates as layer-entry priority ordering. Trigger keys,
scan depth, insertion position, probability, enabled state, and retrieval role
are validated and stored for editor compatibility, but are `stored_only` until a
later scorer/prompt-insertion slice implements them.

`POST /v1/admin/roleplay/lore/entries` creates a manual entry and links it to a
layer. The request accepts `layer_id` / `layerId`, optional `is_constant` /
`isConstant` / `constant`, optional `priority` / `insertion_order`, stable lore
controls, and either a native `write` object or flat entry fields. If
`record_id` is omitted, the service generates one. The service defaults `shape`
to `{ "shape_id": "lore_entry", "version": 1 }`, `source` to `ui`, and adds a
browser-admin UI evidence ref when none is supplied.

```json
{
  "layer_id": "world-main",
  "is_constant": false,
  "priority": 0,
  "write": {
    "record_id": "entry-id",
    "world_id": "world-id",
    "entity_id": "clockmaker",
    "title": "Clockmaker Song",
    "body": "The clockmaker sings at dusk.",
    "canon_status": "draft",
    "visibility": "public",
    "content": {
      "metadata_json": { "tags": ["song"] }
    },
    "evidence_refs": [
      { "evidence_type": "ui", "ref_id": "browser-editor" }
    ],
    "source": "ui",
    "confidence": 1,
    "durability_rationale": "Created by browser editor."
  }
}
```

`PATCH /v1/admin/roleplay/lore/entries/{entryId}` updates editable fields in
place through the native revision-checked replace path. `expected_revision` /
`expectedRevision` is required. The body may provide a nested `write` object or
flat partial fields such as `title`, `body`, `canon_status`, `visibility`,
`entity_id`, `content`, lore controls, `evidence_refs`, `confidence`, and
`durability_rationale`. The response uses the same detail shape as `GET`, so
the frontend can refresh the edited entry directly.

`GET /v1/admin/roleplay/lore/layers/{layerId}/entries/{entryId}` returns one
layer-entry join. `PATCH` on the same route updates layer-scoped controls:
`is_constant` / `isConstant` / `constant` and `priority` / `insertion_order`.
Use this route when an entry is linked to more than one layer and the UI is
editing the controls for a specific layer membership.

`POST /v1/admin/roleplay/lore/entries/{entryId}/promote` promotes an
auto-captured or otherwise lower-layer entry into a durable target layer through
the native promotion path. The body accepts `target_layer_id` / `targetLayerId`,
optional `source_layer_id` / `sourceLayerId`, optional `new_record_id` /
`newRecordId`, optional `is_constant` / `isConstant`, optional `priority`, and
optional `now`. If `new_record_id` is omitted, the service generates one. If
`source_layer_id` is omitted, the service can infer it from `profile_id`,
`chat_id`, or explicit source layer scope when exactly one scoped layer contains
the entry; otherwise the request is rejected with
`roleplay_lore_source_layer_required` or
`roleplay_lore_source_layer_ambiguous`. Archived or readonly targets are
rejected. The response uses the same detail shape as `GET` for the promoted
entry and adds `promoted`, `source`, and `target` fields.

```json
{
  "targetLayerId": "world-main",
  "sourceLayerId": "auto-captured",
  "newRecordId": "clockmaker-song-promoted",
  "isConstant": true,
  "priority": 10
}
```

## Chat Layer Binding

- `GET /v1/admin/roleplay/lore/chat-layers?chat_id=<sessionId>`
- `POST /v1/admin/roleplay/lore/chat-layers`
- `POST /v1/admin/roleplay/lore/chat-layers/toggle`
- `POST /v1/admin/roleplay/lore/chat-layers/reorder`

Set/reorder accepts:

```json
{ "chatId": "session-id", "layerIds": ["world-main", "cast"] }
```

The response includes ordered `activeLayerIds` on reads.

## Characters

- `GET /v1/admin/roleplay/profiles/{profileId}/characters`
- `POST /v1/admin/roleplay/profiles/{profileId}/characters`
- `GET /v1/admin/roleplay/profiles/{profileId}/characters/{characterId}`
- `PATCH /v1/admin/roleplay/profiles/{profileId}/characters/{characterId}`
- `DELETE /v1/admin/roleplay/profiles/{profileId}/characters/{characterId}`

Delete archives the character instead of hard-deleting it. List excludes archived
characters unless `include_archived=true`.

Create accepts:

```json
{
  "id": "hero",
  "name": "Hero",
  "description": "Short public description",
  "personality": "curious",
  "scenario": "Starting scene",
  "firstMessage": "Hello.",
  "alternateGreetings": [],
  "exampleMessages": [],
  "tags": ["player"],
  "avatarUrl": "https://example.invalid/avatar.png"
}
```

## Roleplay Sessions

- `GET /v1/admin/roleplay/sessions?profile_id=<profileId>`
- `POST /v1/admin/roleplay/sessions`
- `GET /v1/admin/roleplay/sessions/{sessionId}`
- `PATCH /v1/admin/roleplay/sessions/{sessionId}`
- `POST /v1/admin/roleplay/sessions/{sessionId}/archive`
- `POST /v1/admin/roleplay/sessions/{sessionId}/restore`
- `GET /v1/admin/roleplay/sessions/{sessionId}/prompt-stack`

Create accepts:

```json
{
  "sessionId": "optional-explicit-id",
  "profileId": "narrator-profile",
  "displayName": "Session title",
  "characterId": "hero",
  "activeLayerIds": ["world-main"]
}
```

Responses include `display_name`, `character_id`, `character_name`,
`active_layer_ids`, `active_layer_count`, `last_message_preview`,
`archived`, timestamps, and raw browser-safe `metadata`.

Normal chat messages still do not create sessions implicitly. `/new` retains
archive-and-create semantics through the existing command/control path.

## ST Packet Import

- `POST /v1/admin/roleplay/imports/st-packet`

The ST packet import route accepts a normalized import plan from a frontend or
importer. The route does not parse arbitrary ST files directly; it owns the
durable write path once the importer has normalized character, persona, lore,
prompt/preset provenance, and transcript rows.

The request supports:

- `profileId`, `importId`, `provenance`, and `rawSource`.
- `character` and `persona` records using the same browser-safe fields as the
  character/persona admin routes.
- `loreLayer` plus `loreEntries`; ST trigger/control metadata is preserved under
  lore entry metadata while supported controls are mirrored into
  `lore_controls`.
- `session` plus `transcriptRows`; each row becomes one message slot, and
  swipes/variants become message variants with active swipe selection.

Response counts include `loreEntries`, `messages`, `assistantVariantRows`,
`assistantMultiSwipeRows`, and total `variants`. The route stores an import
summary and raw provenance in the roleplay import KV scope for later audit or
export work.

`GET /v1/admin/roleplay/sessions/{sessionId}/prompt-stack` returns the
compiled roleplay prompt preview for the session without waking the agent. The
response includes:

- `promptContext`: compatibility string used by the current brain role assembly.
- `stack.compiled_text`: the same compiled prompt text.
- `stack.sections`: ordered prompt sections with source ids, inclusion reasons,
  editability markers, derived markers, and token estimates.
- `stack.trace`: one entry per included section for UI inspection.
- `stack.macro_resolutions`: counts for resolved macros such as `{{char}}` and
  `{{user}}`.
- `stack.messages`: provider-shaped preview messages for future roleplay brain
  migration work.

## Narrator Config

- `GET /v1/admin/roleplay/profiles/{profileId}/narrator-config`
- `PATCH /v1/admin/roleplay/profiles/{profileId}/narrator-config`
- `POST /v1/admin/roleplay/profiles/{profileId}/narrator-config`

Config changes apply on the next wake/config reload boundary and patch only the
`roleplayNarrator` profile section.

```json
{
  "tone": "wry",
  "pacing": "balanced",
  "explicitness": "romantic",
  "memoryDepth": "deep",
  "stylePrompt": "Direct narrator style guidance/instructions",
  "exemplar": "Style exemplar text",
  "review": { "enabled": true, "maxReviewCycles": 2 }
}
```

Allowed values:

- `tone`: `whimsical`, `dramatic`, `matter_of_fact`, `lush`, `wry`
- `pacing`: `leisurely`, `balanced`, `rapid`, `breathless`
- `explicitness`: `implied`, `suggestive`, `romantic`, `steamy`
- `memoryDepth`: `shallow`, `medium`, `deep`
- `stylePrompt`: direct narrator style guidance assembled by UI controls or
  edited by the user. The narrator treats it as instruction text.

### Mechanic profile configuration

- `GET /v1/admin/roleplay/profiles/{profileId}/mechanic-config`
- `PATCH /v1/admin/roleplay/profiles/{profileId}/mechanic-config`
- `POST /v1/admin/roleplay/profiles/{profileId}/mechanic-config`

The mechanic route configures an existing Crew profile as a distinct OOC
diagnostic agent. `name` updates the profile display name and `providerAlias`
uses the normal service model-provider registry; neither value is duplicated
inside a second roleplay-specific model registry. Applying the route selects
the built-in `roleplay_mechanic` local tool profile and its isolated tool
policy.

Narrator profiles cannot be converted in place. The route rejects profiles
that already carry narrator configuration so the narrator and mechanic retain
separate identities, sessions, prompts, and tool policies.

```json
{
  "config": {
    "name": "Maren",
    "providerAlias": "deepseek-flash",
    "autoMonitor": false
  }
}
```

Readback returns normalized `config`, `configured`, `localToolProfileId`,
`toolPolicyIsolated`, and `applies: "next_wake"`. `autoMonitor` is deliberately
reported as `{ enabled: false, available: false, status: "inactive_future" }`.
Requests that enable it fail closed until actual monitoring behavior exists.
Rust owns normalization and the canonical mechanic prompt. Mechanic profiles
are normal full-agent brains; their separate purpose and session association
are represented by the roleplay runtime contracts rather than a parallel LLM
loop.
- `exemplar`: optional reference prose or turns. When both `stylePrompt` and
  `exemplar` are present, `stylePrompt` is binding guidance and `exemplar`
  remains rhythm/density reference material, not prose to copy.
- `review.maxReviewCycles`: integer `0..8`

## Prompt Semantics

When a session has selected character metadata or active lore layers, Rusty Crew
adds a `Roleplay Session Context` section to the next brain role assembly. This
section is session-scoped setup, not a secret-bearing prompt dump. It includes
the selected character's browser-safe fields and active lore layer ids, and tells
the brain to prefer current chat evidence if session metadata conflicts with the
conversation.
