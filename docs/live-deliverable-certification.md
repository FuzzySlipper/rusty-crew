# Live Deliverable Certification

Rusty Crew uses three complementary test layers:

- unit tests for deterministic pure logic;
- smokes for integration paths and runtime contracts;
- live Rusty View certification for substantial chat/runtime deliverables.

Unit tests and smokes prove code paths. They do not prove that a real user can
see and use the behavior in chat. When work changes visible or model-facing
runtime behavior, final deliverability requires a real Rusty Crew backend,
profile, provider, and Rusty View chat client.

The source framework lives in `../rusty-view/docs/live-testing.md`. This doc is
the Rusty Crew-side rule and evidence template.

## When Live Certification Is Required

Run live certification before closing work that changes:

- chat streaming, SSE projection, refresh recovery, or transcript persistence;
- reasoning, tool, command, attachment, activity, or debug blocks;
- profile, session, model, provider, MCP, or command controls;
- service-host routes that Rusty View or a chat client depends on;
- substantial brain, bridge, context, storage, or runtime architecture where a
  live chat path is the real user surface.

Do not close these deliverables based only on unit tests, Rust tests, store
state, curl responses, or smoke scripts. Those checks remain valuable supporting
evidence, but rendered browser output is the primary evidence for UI/chat
claims.

## Normal Run Path

Use the shared Playwright broker. It owns dev-server host/port allocation,
captures run metadata, and prevents agents from killing unrelated processes on
busy ports. Follow Den Services doc
`den-services/playwright-broker-agent-usage` and the local broker guide at
`/home/dev/den-services/playwright-broker/docs/agent-usage.md`.

From `/home/dev/den-services`:

```bash
export DEN_PLAYWRIGHT_BROKER_CONFIG_PATH=/home/dev/den-services/playwright-broker/config/config.example.yaml

go run ./playwright-broker/cmd/den-playwright run rusty-view \
  -repo /home/dev/rusty-view \
  -den-project rusty-view \
  -den-task <task-id> \
  --grep @live-agent \
  --pw-project chromium
```

For headed visual debugging:

```bash
go run ./playwright-broker/cmd/den-playwright run rusty-view \
  -repo /home/dev/rusty-view \
  -den-project rusty-view \
  -den-task <task-id> \
  --grep @live-agent \
  --pw-project chromium \
  --headed
```

Useful live variables:

```bash
RV_LIVE_BACKEND_URL=http://127.0.0.1:9347
RV_LIVE_PROFILE=tester
RV_LIVE_MIN_STREAMING_MS=15000
```

Focused scenarios can use a targeted grep:

```bash
go run ./playwright-broker/cmd/den-playwright run rusty-view \
  -repo /home/dev/rusty-view \
  -den-project rusty-view \
  -den-task <task-id> \
  --grep "@reasoning" \
  --pw-project chromium
```

Local Rusty View scripts are a manual fallback only when the broker is
unavailable or the user explicitly asks for a direct run:

```bash
pnpm e2e:live
pnpm e2e:live:headed
```

## Required Artifacts

Live scenarios write artifacts under Playwright output, normally in a
`live-artifacts` folder. Broker runs also provide
`PLAYWRIGHT_BROKER_ARTIFACT_ROOT`, `PLAYWRIGHT_BROKER_EVIDENCE_PATH`, and
`run-index.json`.

The completion note must cite:

- screenshots inspected;
- visible transcript behavior;
- `debug-snapshot.json`;
- `evidence-packet.json`;
- `scenario-summary.md`;
- trace path when relevant;
- console/page errors when relevant.

Agents must inspect the screenshots and evidence packet before reporting
success. For streaming, inspect an in-progress screenshot as well as the final
response. For controls, compare before/after screenshots and confirm the
rendered region changed.

## Completion Evidence Template

Use this template in Den task comments or completion packets when live
certification applies:

```text
Live scenario:
Command:
Backend/profile/provider:
Artifacts:
Screenshots inspected:
Rendered behavior observed:
Evidence packet:
Timeline notes:
Supporting checks:
Residual risk:
```

If live certification could not run, say why directly and do not mark the
chat/runtime behavior as deliverable. During the architecture remediation window
it is acceptable to restart Rusty Crew, reset local service/profile/provider
state, and recreate the live-test profile or provider setup. The repeatable
tester profile/provider setup is documented in `docs/live-test-profile-setup.md`.

## Reviewer Standard

Reviewers should reject completion claims for substantial chat/runtime work when
the evidence stops at "unit tests passed", "smoke passed", "curl returned OK",
or "store state looked right". Those are supporting checks, not live
deliverable proof.

A closure comment is sufficient only when it either:

- includes the live evidence template with artifact paths and a short
  human/agent inspection summary; or
- explicitly says live certification was not applicable and explains why the
  change has no user-visible or model-visible runtime behavior.

If live certification was applicable but could not run, the task should be left
blocked or deferred with the missing dependency named directly.
