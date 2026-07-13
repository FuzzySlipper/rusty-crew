# External Agent Session Creation

`POST /v1/external-agent-sessions` is the browser-safe path for creating a
Crew-owned session backed by a native Codex app-server thread. Clients do not
pre-create session, agent, or binding identifiers and must not call the lower
level binding API as a substitute.

## Request

```json
{
  "idempotencyKey": "view:create:8ee1f9d7",
  "runtimeId": "codex-local",
  "profileId": "asha-planner",
  "cwd": "/home/dev/asha",
  "taskRef": { "project_id": "asha", "task_id": "4281" },
  "label": "Asha planning agent"
}
```

`idempotencyKey`, `runtimeId`, `profileId`, and `cwd` are required. `cwd` must
be an absolute path that is already normalized. The runtime must be enabled,
observed ready, and held by the current Crew controller. The profile must be
active and permit full sessions.

The success envelope contains:

- `creation`: durable Rust creation state, generated Crew identity, binding,
  native correlation, revision, and phase.
- `runtime`: the runtime registration used for creation.
- `thread`: the bounded browser projection of the native Codex thread.

## Ownership And Recovery

Rust owns request validation, deterministic generated identifiers, the request
fingerprint, session and binding persistence, phase transitions, and revision
fencing. TypeScript owns only the Codex transport operation and browser
projection.

The durable phases are:

1. `prepared`
2. `binding_ready`
3. `native_starting`
4. `recovery_required` when a native operation has an uncertain result
5. `ready`

Crew supplies `threadSource: rusty-crew:<deterministic-hash>` to
`thread/start`. A retry first searches Codex's durable thread catalog for that
marker. This recovers a thread created before a timeout or transport loss and
prevents a second native thread from being created. Completion writes the
native thread ID to the binding before marking the creation ready; a retry can
therefore also repair a process loss between those writes.

On controller restart, persisted bindings are resumed independently. A stale
native thread is reported in the controller's `bindingResumeFailures` array
with its binding ID, native thread ID, reason, and observation time; it does not
degrade an otherwise healthy runtime or block new session creation. An explicit
runtime connect retries binding reconciliation and repairs a stale degraded
registration through exact handshake authorization.

Repeating the same key and intent returns the original creation. Reusing the
key with changed runtime, profile, cwd, task reference, or label fails with
`external_agent_creation_idempotency_conflict`.

## Binding Metadata

After creation, clients update the durable display label and optional Den task
mapping through:

```text
POST /v1/external-bindings/{binding_id}/metadata
```

The body requires `expectedRevision` plus explicit nullable `label` and
`taskRef` fields. Non-empty labels are synchronized to the native Codex thread
name. Clearing `label` restores Crew's unnamed projection; Codex app-server does
not currently expose a native name-clear operation, so clients must treat the
Crew projection as authoritative. Both list and read projections use this
durable metadata, including after service restart.

## Stable Reason Codes

- `external_agent_creation_idempotency_key_required`
- `external_agent_creation_idempotency_conflict`
- `external_agent_creation_runtime_unavailable`
- `external_agent_creation_profile_invalid`
- `external_agent_creation_cwd_invalid`
- `external_agent_creation_revision_conflict`
- `external_agent_creation_binding_conflict`
- `external_agent_creation_native_thread_conflict`
- `external_agent_creation_capacity_conflict`
- `external_agent_creation_native_start_failed`
- `external_agent_creation_recovery_required`

Errors use the standard API error envelope and expose `retryable`. A retryable
failure must use the original request and idempotency key.

## Verification

Focused deterministic coverage lives in the Rust external-runtime engine tests
and `test/external-runtime-controller.test.ts`. The controller test includes a
lost `thread/start` response and proves that recovery finds the one native
thread instead of creating another.

The attached-runtime certification is:

```bash
npm run smoke:external-runtime-service-live -w @rusty-crew/brain-island
```

It creates an active profile, calls the browser endpoint, repeats and conflicts
the request, sends a message through the generated binding, and requires a real
Codex app-server LLM turn to complete with the expected response.

The deployed debug-service metadata certification is:

```bash
npm run smoke:external-binding-metadata-live-debug-service -w @rusty-crew/brain-island
```

It verifies create, rename, Den remapping, stale revision rejection, clear,
restart persistence, native thread naming, and cleanup against port `9348`.
