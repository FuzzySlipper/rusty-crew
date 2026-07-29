# Provider-Level Wake Cancellation

Task: #4475, updated by #6372

Rusty Crew exposes explicit cancellation for an active logical turn. The
service passes that cancellation signal to buffered Rust brain runs, which
cancel pending provider/tool work, reject late stream output, and publish a
terminal cancellation outcome.

There is no elapsed-time cancellation for a whole wake. Healthy work may cross
any number of scheduling quanta; each yield persists continuation state and
resumes the same logical turn.

Provider request deadlines are a separate, optional operation bound. A request
deadline failure is returned as provider evidence and must not be interpreted
as the lifetime of the surrounding turn.

Deterministic cancellation coverage:

```bash
npm run smoke:openai-responses-cancellation -w @rusty-crew/brain-island
```

The smoke aborts a buffered Responses run explicitly and verifies that late
provider output is not appended after cancellation.
