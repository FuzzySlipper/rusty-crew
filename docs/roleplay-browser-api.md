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
  "exemplar": "Style exemplar text",
  "review": { "enabled": true, "maxReviewCycles": 2 }
}
```

Allowed values:

- `tone`: `whimsical`, `dramatic`, `matter_of_fact`, `lush`, `wry`
- `pacing`: `leisurely`, `balanced`, `rapid`, `breathless`
- `explicitness`: `implied`, `suggestive`, `romantic`, `steamy`
- `memoryDepth`: `shallow`, `medium`, `deep`
- `review.maxReviewCycles`: integer `0..8`

## Prompt Semantics

When a session has selected character metadata or active lore layers, Rusty Crew
adds a `Roleplay Session Context` section to the next brain role assembly. This
section is session-scoped setup, not a secret-bearing prompt dump. It includes
the selected character's browser-safe fields and active lore layer ids, and tells
the brain to prefer current chat evidence if session metadata conflicts with the
conversation.
