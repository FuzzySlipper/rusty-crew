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
