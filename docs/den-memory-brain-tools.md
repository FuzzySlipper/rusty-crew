# Den Memory Brain Tools

Task: Den `2899`

Rusty Crew now has brain-island tool factories for the Den Memories tool family:

- `den_memory_recall`
- `den_memory_read`
- `den_memory_search`
- `den_memory_store`
- `den_memory_propose`

These tools wrap the `@rusty-crew/adapter-den` Den Memories client. They do not
make Rusty Crew the owner of Den memory data.

## Policy Modes

The tool context takes an explicit `DenMemoryToolPolicy.mode`:

- `off`: all Den memory tools return policy-denied results.
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
