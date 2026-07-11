# Codex App-Server 0.144.1 Live Semantics Spike

Date: 2026-07-10

Den task: `rusty-crew#5517`

## Decision

Proceed with an **attached Unix WebSocket** controller for the first Rusty Crew
Codex lane. The app-server remains an independently supervised process using the
normal user `CODEX_HOME`; Crew connects to its Unix socket, owns one operational
controller connection, and resumes durable Codex thread IDs after either process
is replaced.

Managed stdio remains useful as a compatibility oracle and emergency diagnostic
transport. It is not the shipping transport: tying app-server lifetime to Crew
would discard the restart/adoption behavior that the live Unix service already
provides. TCP WebSocket is out of scope.

The first implementation should therefore use:

- the supervised `codex app-server --listen unix://...` process;
- WebSocket JSON-RPC over its Unix socket;
- one Crew controller connection that multiplexes threads and routes requests by
  `threadId`, `turnId`, and request ID;
- exact Codex thread IDs as external-runtime identities, not Crew-derived IDs;
- explicit `local` environment, cwd, model, effort, approval, and sandbox values
  at thread/turn boundaries;
- fail-closed handling for unknown server requests.

## Installed Runtime

The spike used the installed runtime, not `/home/research/codex` and not a fork.

| Artifact | Value |
| --- | --- |
| CLI | `codex-cli 0.144.1` |
| Native executable SHA-256 | `a96f944d1a596dbfb7fdd84f482be5c50e34b04bb371126840d873e4ebf26902` |
| Launcher SHA-256 | `134063e133f0b4244fa3b251acf973d4fe4b4aeeacbdc135211bf480f59f1477` |
| Generated TypeScript | 671 files, `c0dd4f64e872b2cbca9d1733b45a273c3a0f8f486f0ea8122c31b197e8dc092d` |
| Generated JSON Schema | 337 files, `ecb425cf4f71fc5e753bb5f897633b2a9c96a46d8089662adc4c64ba8604819f` |
| Socket | `/run/user/1001/codex-app-server/app-server.sock` |
| `CODEX_HOME` | `/home/agent/.codex` |
| Selected advertised default model | `gpt-5.6-sol`, medium effort |

The service is user-supervised with restart-on-failure. Its systemd sandbox has
`PrivateTmp=true`; host `/tmp` is consequently the wrong location for a Codex
workdir. The probe uses `/home/agent/.cache`, while real service agents should use
their actual `/home/...` repository or configured workdir.

## Transport Findings

The Unix listener speaks WebSocket over UDS, not newline-delimited JSON. A narrow
`ws` client connected through `ws+unix://...:/` successfully. Compression must be
disabled (`perMessageDeflate: false`) because app-server 0.144.1 rejects the
default `sec-websocket-extensions` offer.

`codex app-server proxy --sock` is a raw byte relay and does not perform the
WebSocket handshake required by this listener. It is not a suitable Crew
transport adapter.

A separate current-version `codex app-server --stdio` control initialized and
read the Unix-created thread using the same `CODEX_HOME`. This confirms shared
persistence and protocol compatibility, but adds no durability advantage.

## Live Evidence

Run ID: `a405c8bb-56b1-435d-b7c7-4d3d0249f9a6`

The rerunnable probe is `tools/codex-app-server-live-spike.mjs` and is registered
in the service-host package for `npm run smoke -- --list` discovery. The
successful run used:

```bash
CODEX_APP_SERVER_RESTART_SERVICE=1 \
CODEX_APP_SERVER_KEEP_SCRATCH=1 \
CODEX_APP_SERVER_TURN_TIMEOUT_MS=300000 \
npm run smoke:codex-app-server-live-spike -w @rusty-crew/service-host
```

The probe established all of the following against the live provider and
installed app-server:

- A non-ephemeral thread called a Crew-supplied experimental dynamic tool, read
  Den task `5517` through Den MCP, edited a real git fixture, created a file, and
  ran `npm test` successfully.
- A second turn resumed exact thread
  `019f4f24-2833-7b82-8f3c-e5895cbc580b` after a completed app-server restart.
- The dynamic `rusty_crew.echo_probe` tool remained available after resume.
- A second ephemeral thread shared the same controller connection and exercised
  command approval. The client declined and the attempted host write did not
  occur.
- Plan mode emitted `item/tool/requestUserInput`; the client answered and the
  turn completed.
- `turn/steer`, `turn/interrupt`, and `thread/compact/start` behaved as expected.
- A real Codex subagent emitted `collabAgentToolCall` and `subAgentActivity`
  items.
- During a pending dynamic-tool callback, app-server PID `1991476` was killed
  with `SIGKILL` and replaced by PID `1996820`. Resume reconstructed turn
  `019f4f26-0cd3-74d0-93f2-8dc47b229030` as `interrupted`, not falsely active.
- No unknown server request was observed. The probe would answer any unknown
  request with JSON-RPC `-32601` rather than guessing.

Representative sanitized protocol envelopes are:

```json
{"method":"initialize","id":1,"params":{"clientInfo":{"name":"rusty_crew_live_spike","version":"0.1.0"},"capabilities":{"experimentalApi":true}}}
{"method":"item/tool/call","id":"<server-request-id>","params":{"threadId":"<thread-id>","turnId":"<turn-id>","namespace":"rusty_crew","tool":"echo_probe","arguments":{"token":"<probe-token>"}}}
{"id":"<server-request-id>","result":{"contentItems":[{"type":"inputText","text":"RUSTY_CREW_DYNAMIC_ACK:<probe-token>"}],"success":true}}
{"method":"turn/interrupt","id":"<request-id>","params":{"threadId":"<thread-id>","turnId":"<turn-id>"}}
```

Observed completed item types were `agentMessage`, `collabAgentToolCall`,
`commandExecution`, `contextCompaction`, `dynamicToolCall`, `fileChange`,
`mcpToolCall`, `reasoning`, `subAgentActivity`, and `userMessage`.

## Capability Matrix

| Capability | Result |
| --- | --- |
| Initialize/model discovery | Proven |
| Non-ephemeral code-changing thread | Proven |
| Host-side validation command | Proven |
| Den MCP call | Proven |
| Dynamic Crew tool callback | Proven |
| Dynamic tool after process replacement | Proven |
| Multiple threads on one controller | Proven |
| Exact-ID resume | Proven |
| Command approval | Proven, declined safely |
| `requestUserInput` | Proven |
| Steer | Proven |
| Interrupt | Proven |
| Manual compaction | Proven |
| Codex subagent events | Proven |
| Pending callback during hard kill | Proven; reconstructed interrupted |
| File-change approval request | Not emitted by this scenario |
| Granular permission request | Prompted but not emitted by this model/runtime |
| MCP elicitation | No configured MCP interaction required elicitation |

The three non-observed callbacks must remain implemented as fail-closed broker
branches and receive focused fixtures or a future live scenario when an installed
runtime can deterministically emit them. Their absence does not justify inventing
wire behavior or blocking the attached-Unix architecture.

## Implementation Constraints

1. Treat the operational WebSocket connection as a controller lease. Only one
   Crew controller should answer server callbacks for a given app-server.
2. Route notifications and requests by exact external IDs. Do not infer active
   thread ownership from arrival order.
3. A Crew restart should reconnect and resume. An app-server hard failure may
   interrupt an active turn; Crew must project that terminal status rather than
   replaying the turn automatically.
4. Pending dynamic-tool calls are not recoverable across app-server death. Tool
   effects need Rusty Crew idempotency where retries are ever introduced.
5. Bind `environments: [{ environmentId: "local", cwd }]` explicitly. Omitting
   it selects mutable account defaults; an empty list removes execution tools.
6. Keep generated exact-version protocol artifacts or fingerprints under CI
   control so a Codex upgrade cannot silently alter the bridge contract.
7. Preserve normal Codex configuration discovery. Crew should add identity-bound
   dynamic tools and lifecycle projection without replacing Codex's native MCP,
   skills, hooks, AGENTS, model, or subagent machinery.

## Recommendation

**Proceed** to the transport/ownership ADR and attached-Unix implementation.
The durable path is materially better than managed stdio for this service model,
and the live run found no reason to build a temporary stdio architecture first.
Keep stdio as a diagnostic control, not a legacy fallback in production routing.
