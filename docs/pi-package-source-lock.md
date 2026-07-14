# Retired pi Package Source Lock

Rusty Crew no longer uses the `earendil-works/pi` npm packages as runtime
dependencies. The production Chat Completions brain is a native Rust loop with
the canonical module id `chat-completions`. Retired Pi-named brain ids are not
aliases and are rejected after migration.

This file is retained as historical audit context for work that preceded the
Rust brain cutover. Do not add these packages back to manifests to satisfy a
current feature.

Existing service roots from before the rename must migrate profile JSON before
starting the new binary:

```bash
npm run migrate:chat-completions-brain-config -- /path/to/service-root
```

The command parses and atomically rewrites profile JSON and is idempotent.
SQLite schema 46 and PostgreSQL schema 31 migrate profile-registry rows in a
backend transaction and discard obsolete provider wire state, which the Chat
Completions brain does not use. The runtime does not alias the retired id.

## Retired Pin

- Repository: `https://github.com/earendil-works/pi`
- Commit: `6e6ce70caf3328683517b0e308fdbbc6d1c1abc9`
- `@earendil-works/pi-agent-core`: `0.79.8`
- `@earendil-works/pi-ai`: `0.79.8`

Older local checkout paths in audit docs are historical context only.
