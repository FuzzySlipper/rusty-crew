# External Memory Brain Tools

Task: Den `2899`

Rusty Crew exposes model-callable tools for a configured external memory service:

- `memory_recall`
- `memory_read`
- `memory_search`
- `memory_store`
- `memory_propose`

The current backend adapter is Den-owned and remains visible in diagnostics and
evidence. Model-facing prompts and descriptions deliberately call this surface
external memory: it is not Den documents, tasks, projects, or guidance.

## Policy Modes

The tool context takes an explicit `DenMemoryToolPolicy.mode`:

- `off`: all external memory tools return policy-denied results.
- `metadata`: read/search/recall are allowed; store/propose are denied.
- `candidate`: store requests are routed to `propose`.
- `manual`: store is denied with a manual-review reason; propose remains
  available.
- `permissive`: store writes directly only for allowed full/prime contexts;
  other contexts fall back to proposal.

The default direct-store context is a `full` session kind or a configured
profile id such as `prime`. Worker/review-style contexts propose instead of
storing directly.

## Runtime Context

Tool calls preserve Rusty Crew runtime context when provided:

- project id
- task id
- session id
- agent id
- profile id
- run id

The tools also preserve audience, role, mode, source refs, and metadata in
client requests.

## Tool Results

All tools return JSON details with:

- `ok`
- `operation`
- `mode`
- `action`
- optional `reasonCode`
- optional `retryable`
- optional raw client result

This keeps model-facing output and diagnostics aligned.

## Verification

`npm run smoke:den-memory-tools` covers off, metadata, candidate, manual, and
permissive behavior; direct store for full/prime contexts; store-to-proposal
fallback; missing client diagnostics; and runtime metadata preservation.
