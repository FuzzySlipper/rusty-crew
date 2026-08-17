# Codex External Runtime Northstar Certification

Task: #5530  
Date: 2026-07-11  
Verdict: **Revise and continue; Crew substrate proceeds, production cutover does not.**

## Scope Proven In This Slice

The rerunnable live service smoke uses the installed Codex app-server 0.144.1,
its supervised Unix WebSocket, normal `/home/agent/.codex`, a disposable SQLite
Crew engine, and the real provider. Run it with app-server replacement enabled:

```bash
CODEX_APP_SERVER_RESTART_SERVICE=1 \
  npm run smoke:external-runtime-service-live -w @rusty-crew/brain-island
```

The 2026-07-11 acceptance run proved:

- one binding retained native thread
  `019f502f-ffa0-7911-9b8a-652229bcb4d4`;
- the initial turn completed as
  `019f5030-0185-7b23-8ba2-fb491db443b2`;
- two messages admitted before dispatch became two ordered native turns,
  `019f5030-0c6b-7793-bc71-552a12e6a3b3` then
  `019f5030-11f7-72d3-8844-92563e3d6074`;
- replaying the second HTTP delivery returned the existing durable receipt and
  did not create a third queue item or turn;
- steer and interrupt completed against exact native turn IDs
  `019f5030-1851-7413-b707-a4113aa6edbc` and
  `019f5030-34c4-7363-9a1e-985f81fe7d44`;
- SSE replay returned assistant deltas and turn lifecycle events;
- the supervised app-server was replaced between committed turns;
- a fresh Crew engine and controller reacquired the lease at generation 2
  after generation 1 stopped, resumed the exact native thread, and completed
  turn `019f5030-38d1-7aa1-9bf2-ba1aab62a86d`;
- terminal turns released their capacity lease;
- deterministic Rust coverage proves expired queued follow-ups become terminal
  expired records and never materialize an external turn.

A second live run then proved a Codex-to-Codex correlated round over two bound
threads on the same controller:

- correlation: `codex-codex-1783757467934`;
- sender thread: `019f503a-cfbc-7620-bbe0-154938d667c8`;
- sender turn: `019f503a-f123-70c3-ab56-ec077cce43db`;
- peer thread: `019f503a-d275-7c20-a91a-b23cf1b6c4db`;
- peer turn: `019f503b-3490-7c21-8691-fbb1540fc133`;
- `agent_round` tool call:
  `exec-53ab706e-8847-4359-b3e3-c694e55a626a`;
- peer `send_agent_message` call:
  `exec-e1c4912c-75fb-4bc2-a2a1-060a5e1ea0ad`;
- durable Crew round:
  `codex-round:codex-service-live-binding:019f503a-cfbc-7620-bbe0-154938d667c8:019f503a-f123-70c3-ab56-ec077cce43db:exec-53ab706e-8847-4359-b3e3-c694e55a626a`,
  terminal status `replied`.

This run found and fixed receive-head-of-line blocking in the Codex driver.
Known server requests now resolve concurrently with response and notification
decoding, so a pending `agent_round` callback cannot prevent the same socket
from receiving the response that starts the recipient turn. Unit coverage holds
a server request open while a client `thread/list` response completes.

The HTTP replay defect found during this run was fixed at the service boundary:
server-generated timestamps no longer make a semantically identical retry
conflict with its durable request fingerprint. Conflicting reuse still reaches
Rust authority and fails closed.

## Direct-Brain Coexistence

After deploying the same checkout to the debug service, the cross-runtime live
harness passed both code-edit and structured-readback scenarios through Codex
app-server and the direct Responses brain:

- run ID: `capability-1783756871337-541377d6`;
- artifact root: `/tmp/rusty-crew-capability-5530`;
- Codex thread: `019f5031-d816-7ee0-b1e1-e757324e365f`;
- Responses session: `responses-cert-5389-session`.

The direct Chat Completions path also completed a live debug-service turn:

- session: `tester-session`;
- client message: `brain-coexist-1783756942946`;
- replay cursor: `tester-session:190` through `tester-session:236`;
- exact marker observed and terminal status `completed`.

## Storage And Static Gates

- `npm run verify:offline` passed.
- `npm run test:postgres-backend` passed against the configured local
  PostgreSQL service.
- The external-runtime SQLite lifecycle and live restart path passed.
- Queue promotion is backend-neutral Rust engine logic over the shared store;
  PostgreSQL queue/external-runtime repositories passed the full Postgres gate.

## Remaining Northstar Gates

This task must remain open. The following acceptance items are not replaced by
the evidence above:

- Rusty View #5526, #5527, #5528, and final browser workflow #5529 are still
  planned, so no code-changing Den task has yet been completed from Rusty View
  without a CLI/TUI and no screenshot-first fleet-attention proof exists.
- Capability follow-up #5657 still owns service/browser interaction,
  compaction, and broader restart scenarios.
- Capability follow-up #5658 still owns live direct-brain-to-Codex,
  Codex-to-direct-brain, and pending-round restart evidence. Codex-to-Codex is
  now proven above and should be reused rather than rerun as a separate format.
- The current run did not deliberately inject an incompatible live server
  fingerprint; generated protocol drift checks and deterministic fail-closed
  handshake tests pass, but the northstar asks for a live diagnostic capture.

The correct production recommendation is therefore **revise and continue**:
the attached-Unix Crew runtime, persistence, queue, controls, replay, and direct
brain coexistence are credible enough to build on; the operator replacement
claim waits for the Rusty View and cross-agent live gates rather than being
inferred from service-only evidence.
