# External Cassette Fixtures

Status: active testing convention

This directory stores redacted response-shape evidence captured from external
systems such as Den successor services, provider APIs, and Rusty View live-test
routes. Cassettes are deterministic fixtures for offline shape validation. They
are not substitutes for live certification when a task changes user-visible
runtime behavior.

## Directory Shape

Use one directory per external system:

```text
fixtures/external-cassettes/
  den-successor-gateway/
    <scenario>.redacted.json
```

Each cassette should include:

- `schemaVersion`: fixture schema version, currently `1`;
- `source`: external system identifier;
- `capturedAt`: ISO timestamp or a stable fixture timestamp;
- `redaction`: short statement of what was removed or normalized;
- `interactions`: request method/path plus redacted response status/body.

Do not commit request or response headers, bearer tokens, API keys, provider raw
token payloads, OAuth secrets, cookies, or full model prompts. If the raw capture
contains sensitive or overly-large provider text, replace it with a short shape
representative value and mention that in `redaction`.

## Refresh Procedure

1. Capture the live response with the relevant live/debug-service smoke or a
   small one-off curl command.
2. Remove headers and secrets before writing the fixture.
3. Normalize volatile values only when they are not part of the contract being
   checked.
4. Run the cassette smoke for the owning package.
5. If the behavior is user-visible, still attach live Rusty View certification
   evidence to the Den task.

Offline cassettes may run in `verify:offline` when they do not need a native
build, service startup, Den, local Postgres, Rusty View, Telegram, or a real
provider.
