# Task 6003 Chat Completions Reasoning Live Certification

Date: 2026-07-19 (America/Los_Angeles)

## Target

- Service: `rusty-crew-debug.service`
- API: `http://127.0.0.1:9348`
- Storage: dedicated SQLite database under `/home/system/rusty-crew-debug`
- Provider alias: `kimi-k2.7`
- Wire dialect: `kimi`
- Reasoning history: `preserve_all`

The certification rejects any API port other than `9348` and any provider alias
other than `kimi-k2.7`. It creates a disposable `code_read` profile through the
official admin API and hard-deletes the profile when the scenario finishes.

## Scenario

The first user turn instructed the live model to choose and execute two tools in
sequence: `git_status`, then `read_file` after the first result. Durable chat
events showed both successful tool completions in that order. The Rust Chat
Completions loop made three provider requests: one before each tool round and a
third to produce the final answer.

The bounded provider-request debug detail showed that the second request
replayed the first assistant tool-call message with its exact nonempty
`reasoning_content`. The third request retained that same value. A second user
turn then completed without tools; its first provider request restored the same
reasoning value from the persisted `preserve_all` provider state.

Raw reasoning was neither printed nor written into this evidence. Equality is
represented by the SHA-256 digest below.

## Result

```json
{
  "baseUrl": "http://127.0.0.1:9348",
  "providerAlias": "kimi-k2.7",
  "firstWakeProviderRequests": 3,
  "firstWakeTools": ["git_status", "read_file"],
  "firstReasoningSha256": "2dfe3bcf031c45c8c7fd180a3b94cb757eddf8269fe5543573e159ee7910b430",
  "secondWakeProviderRequests": 1,
  "secondWakeRestoredReasoningSha256": "2dfe3bcf031c45c8c7fd180a3b94cb757eddf8269fe5543573e159ee7910b430",
  "rawReasoningPersistedInEvidence": false
}
```

## Command

```bash
npm run smoke:chat-completions-reasoning-live-debug-service -w @rusty-crew/brain-island
```
