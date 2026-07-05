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
- `GET /v1/admin/roleplay/lore/entries/search`
- `POST /v1/admin/roleplay/lore/entries`
- `GET /v1/admin/roleplay/lore/entries/{entryId}`
- `PATCH /v1/admin/roleplay/lore/entries/{entryId}`

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
    "body": "..."
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
  "layerEntries": [{ "layer_id": "world-main", "record_id": "entry-id" }],
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

`POST /v1/admin/roleplay/lore/entries` creates a manual entry and links it to a
layer. The request accepts `layer_id` / `layerId`, optional `is_constant` /
`isConstant`, optional `priority`, and either a native `write` object or flat
entry fields. If `record_id` is omitted, the service generates one. The service
defaults `shape` to `{ "shape_id": "lore_entry", "version": 1 }`, `source` to
`ui`, and adds a browser-admin UI evidence ref when none is supplied.

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
`entity_id`, `content`, `evidence_refs`, `confidence`, and
`durability_rationale`. The response uses the same detail shape as `GET`, so
the frontend can refresh the edited entry directly.

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
