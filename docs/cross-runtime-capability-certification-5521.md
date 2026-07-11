# Cross-Runtime Capability Certification

Task: #5521  
Date: 2026-07-10

## Harness

The reusable harness lives in `@rusty-crew/capability-harness`. A scenario
declares its fixture, prompt, required capabilities, permitted effects,
expected artifacts, and independent validation commands once. Runtime adapters
produce the same normalized evidence shape and the artifact writer emits:

- `debug-snapshot.json` with bounded, narrowly redacted native evidence;
- `evidence-packet.json` with normalized runtime results and comparison facts;
- `scenario-summary.md` with concise human-readable outcomes.

The harness compares capability evidence, artifacts, validation, intervention,
latency, usage signals, and recovery state. It does not compare response prose.
Unsupported and unexercised capabilities remain explicit values rather than
being inferred as success.

## Live Substrates

- Codex app-server 0.144.1 through the supervised Unix WebSocket at
  `/run/user/1001/codex-app-server/app-server.sock`;
- direct Rusty Crew Responses brain through debug service
  `http://127.0.0.1:9348`, profile `responses-cert-5389`, provider
  `responses-proxy-cert-5389`;
- disposable fixtures beneath `/home/.tmp`, which is visible to both the
  app-server service and the Rusty Crew service;
- artifact root `/tmp/rusty-crew-capability-debug`.

Run:

```bash
RUSTY_CREW_CAPABILITY_ARTIFACT_ROOT=/tmp/rusty-crew-capability-debug \
  npm run smoke -- cross-runtime-live
```

Run ID: `capability-1783752965631-4e20f135`  
Codex thread: `019f4ff6-4139-7a40-97d9-0f44a4df3522`

## Focused Code Edit

Both runtimes received equivalent disposable fixtures. Each changed only the
`value` field in `value.json` from `before` to `after`, ran `node test.mjs`,
and returned the requested completion marker.

| Runtime | Duration | Validation | Interventions |
| --- | ---: | --- | ---: |
| Codex app-server | 9,609 ms | `FIXTURE_TEST_OK` | 0 |
| direct Responses | 14,119 ms | `FIXTURE_TEST_OK` | 0 |

Codex evidence included command and file activity. Direct Responses evidence
included `read_file`, `write_file`, and `terminal` tool lifecycles. Independent
post-turn validation read the resulting file and ran the test again for both
fixtures.

## Structured Readback

Each runtime continued on its existing thread/session, read the prior result,
made no further file change, and returned exactly
`CAPABILITY_READBACK_OK:after`.

| Runtime | Duration | Validation | Interventions |
| --- | ---: | --- | ---: |
| Codex app-server | 5,220 ms | `FIXTURE_TEST_OK` | 0 |
| direct Responses | 5,656 ms | `FIXTURE_TEST_OK` | 0 |

## Defect Found

The first successful Codex edit exposed a normalization defect. Generic
`item/started` and `item/completed` notifications containing a
`commandExecution`, `fileChange`, `mcpToolCall`, or `dynamicToolCall` item were
classified as generic item lifecycle events because the mapper considered the
method name but not `item.type`.

The mapper now classifies those item families from the typed item payload, and
unit coverage pins all four mappings. The final live run consequently reports
Codex command and file activity through the normalized evidence packet.

## Current Boundary

This certification satisfies the first harness acceptance: two real scenarios
ran through two runtime paths, including a code-changing validated scenario.
Restart, interaction, subagent, compaction, web/MCP, and cross-runtime
coordination scenarios remain separate follow-up coverage. The packet schema
already represents recovery, interactions, usage, unsupported capabilities,
and backend-native raw evidence so those scenarios do not need a new evidence
format.

## Expanded Capability Run (#5656)

Run ID: `capability-1783764439078-96ad971b`

Artifact root: `/tmp/rusty-crew-capability-5656`

Codex thread: `019f50a5-51d7-7b23-9830-76db1ee817da`

The shared scenario catalog and artifact writer now cover eight live scenarios:

| Scenario | Codex app-server | direct Responses |
| --- | --- | --- |
| focused code edit | supported | supported |
| second-turn structured readback | supported | supported |
| multi-file repo instructions and validation | supported | supported |
| Den MCP task read and thread write | supported | unsupported: no MCP binding |
| web source read | supported | supported |
| background command completion | supported | supported |
| local visual input | supported | unsupported: text-only chat API |
| subagent delegation and result use | supported | supported |

The Den workflow read task `#5656` through native Codex MCP tools and wrote task
message `21186` with the run-specific marker. The direct Responses runtime was
not invoked for that scenario because the certification profile deliberately
has no MCP binding; its evidence packet records the unsupported reason. The
same explicit unsupported handling applies to direct-brain local image input.

The multi-file scenario independently verified both changed files with
`node multi-test.mjs` for each runtime. Background commands, web access, image
inspection, and delegation require both the expected response marker and a
matching observed tool/capability signal. No scenario uses response prose
equality as its comparison metric.

## Lifecycle And Recovery Run (#5657)

Run ID: `lifecycle-1783765641318-04735625`

Artifact root: `/tmp/rusty-crew-capability-5657`

Codex thread: `019f50b7-aa2a-7910-9976-4468fa10c46d`

The lifecycle suite uses the same evidence packet and artifact writer:

- Codex plan mode emitted a real `item/tool/requestUserInput` server request;
  the controller selected `blue`, recorded native request ID `1`, and the turn
  completed with `CAPABILITY_INPUT_OK:blue`.
- Explicit `thread/compact/start` created a native compaction turn. After that
  turn completed, a second turn recalled the pre-compaction marker.
- `turn/steer` used the exact active turn ID and changed the terminal response;
  a separate `turn/interrupt` stopped a live command turn.
- The supervised app-server process was replaced, a fresh driver reconnected,
  and `thread/resume` returned the exact original thread before marker recall.
- `rusty-crew-debug.service` was restarted, and the direct Responses profile
  resumed the exact existing session with its pre-restart marker intact.
- Approval/MCP elicitation is explicitly unsupported in this deployment:
  approval policy is `never` and no configured MCP server advertises an
  elicitation flow. Neither runtime receives synthetic success for it.

The restart evidence populates `restart.exercised`, `restart.recovered`, and a
specific thread/session evidence string. Control and structured-input calls are
recorded in `interactions` with their native identities.

## Direct Messaging And Correlated Rounds (#5658)

Run ID: `coordination-1783767972204-6dd0bcce`

Artifact root: `/tmp/rusty-crew-capability-5658`

The live coordination suite uses the deployed debug service, Pi-backed
`tester` agent, two supervised Codex bindings, and the shared Rust-owned
delivery/round substrate. It asserts durable identities and terminal states,
not assistant response prose.

- direct Pi agent -> Codex completed round
  `round:tester-session:service-tester-session-1783767972332-3:call_01_769DgQ9mDjHvX7z7xvSp5667`;
- Codex -> direct Pi agent completed round
  `codex-round:rv-codex-5516-a-binding:019f5085-b337-7740-97da-4b25d86bde41:019f50db-7739-77b3-8096-54880d229e16:exec-a52a4d62-1ddf-480c-b836-cf7d6b2da290`;
- Codex -> Codex completed round
  `codex-round:rv-codex-5516-a-binding:019f5085-b337-7740-97da-4b25d86bde41:019f50db-b1c6-76d2-8c71-575bb7d6b7a0:exec-5b901e38-c5fe-496a-9bdf-c289de918b2c`;
- each replied round preserves sender/recipient agent and session IDs,
  correlation ID, Crew message/round ID, and Codex thread/turn/tool-call IDs;
- replaying the pending-restart trigger returned the same durable delivery
  receipt instead of creating another turn;
- restarting `rusty-crew-debug.service` advanced controller generation 8 to 9
  while preserving the exact pending round record;
- that unrecoverable in-flight native callback terminated as `expired` with
  `agent_round_timeout` at its 12-second TTL, with no reply message and no
  resurrected work.

The live run exposed and fixed two service-host timing gaps. Codex dynamic-tool
deliveries now prompt an immediate wake-event drain when Rust selects a direct
brain target. Manual chat/debug dispatch also registers duplicate-bus-event
suppression before awaiting the LLM turn, preventing the background drain from
leaving a stale suppression marker that could swallow the next legitimate
cross-agent wake.
