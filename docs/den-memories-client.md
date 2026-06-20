# Den Memories Client

Task: Den `2898`

Rusty Crew now has a typed Den Memories client in `@rusty-crew/adapter-den`.
This client is the platform/data integration layer for future brain-island
memory tools.

## Surface

The client exposes:

- `read`
- `search`
- `recall`
- `store`
- `propose`

Requests preserve Rusty Crew runtime context fields when supplied:

- `projectId`
- `taskId`
- `sessionId`
- `agentId`
- `profileId`
- `runId`

Requests also support audience, role, mode, source refs, and arbitrary metadata
where applicable.

## Deployment Configuration

`createDenMemoryClient` accepts:

- `baseUrl`
- optional bearer token
- optional fetch implementation
- timeout in milliseconds
- optional endpoint path overrides

Default paths are under `/v1/memories/*`, but deployments can override every
path. Brain tools should depend on this client interface, not hardcoded Den
deployment routes.

## Errors

Failures throw `DenMemoryClientError`, carrying:

- `code`
- HTTP `status`
- Den `reasonCode`
- `retryable`

This shape is meant to be usable both in model-facing tool output and operator
diagnostics.

## Boundary

Den memory records remain Den-owned external memory. The client does not write
to Rust coordination persistence. Rust should see memory use through selected
tool descriptors and bounded tool telemetry only.

## Verification

`npm run smoke:den-memory-client` uses a fake Den Memories server to cover path
overrides, bearer auth, read/search/recall/store/propose calls, context/source
metadata preservation, and typed error propagation.
