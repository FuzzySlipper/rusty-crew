# Rust Pi-Agent Live Certification Evidence - Task 4565

Date: 2026-07-07

This note records the first live certification of the Rust `pi-agent-core`
brain path after the TypeScript pi-agent internals were retired.

## Target

- Rusty Crew debug service: `http://127.0.0.1:9348`
- Service root: `/home/system/rusty-crew-debug`
- Storage: SQLite debug database
- Profile: `tester`
- Session: `tester-session`
- Provider alias: `tester-chat`
- Provider route: local den-router chat completions, `deepseek-flash`
- Local tool profile: `full_agent`

At the time of this historical certification, the debug service used a live
toggle that no longer exists. Production hosts now always use the live Rust
path. Only the long idle budget remains relevant:

```text
RUSTY_CREW_PI_AGENT_STREAM_IDLE_TIMEOUT_MS=30000
```

After setting them in `/home/system/rusty-crew-debug/config/service.env`, the
debug service was restarted and `GET /v1/admin/healthz` returned healthy.

## Direct Service Probes

The probes used `POST /v1/chat/sessions/tester-session/messages` against the
debug service and then inspected the chat event readback API.

### No-Tool Turn

- Prompt marker: `rustcutover`
- Wake id: `service-tester-session-1783425807140-1`
- Observed events: `assistant_text_delta`, `assistant_turn_finished`,
  `assistant_message_completed`
- Result text included: `certification no-tool check ... is acknowledged with
  rustcutover`

This verified that the live provider path was active instead of the fake bridge
diagnostic response.

### Tool Turn

- Prompt marker: `toolrustcutover`
- Requested tool: `read_file`
- Requested path: `/home/dev/rusty-crew/package.json`
- Wake id: `service-tester-session-1783425825486-2`
- Observed events: `assistant_reasoning_delta`, `tool_call_started`,
  `tool_call_completed`, `assistant_text_delta`, `assistant_message_completed`
- Tool result: `read_file`, `is_error=false`
- Result text included: `Package name: rusty-crew` and `toolrustcutover`

This verified a model-visible neutral tool round through the Rust pi-agent brain
and TS tool executor.

### Failure/Recovery Turn

- Prompt marker: `failrustcutover`
- Requested tool: `read_file`
- Missing path:
  `/home/dev/rusty-crew/definitely-missing-live-cert-1783425848.txt`
- Wake id: `service-tester-session-1783425848501-3`
- Observed events: `assistant_reasoning_delta`, `tool_call_started`,
  `tool_call_failed`, `assistant_text_delta`, `assistant_message_completed`
- Tool result: `read_file`, `is_error=true`
- Result text reported the missing file and recovered gracefully.

This verified that a live tool failure can be surfaced back to the model without
killing the turn.

## Rusty View Rendered Proof

The existing broker live fixture currently configures setup calls with
`RV_LIVE_BACKEND_URL` but did not pass that backend into the rendered Rusty View
app. A manual Rusty View run was used for this certification so the app itself
opened the debug service via the supported `api` query parameter:

```text
http://127.0.0.1:37102/?api=http%3A%2F%2F127.0.0.1%3A9348
```

Manual artifacts:

- Directory: `/tmp/rusty-crew-live-cert-4565`
- Final screenshot:
  `/tmp/rusty-crew-live-cert-4565/04-completed.png`
- In-progress screenshot:
  `/tmp/rusty-crew-live-cert-4565/03-streaming.png`
- Debug snapshot:
  `/tmp/rusty-crew-live-cert-4565/debug-snapshot.json`
- Evidence summary:
  `/tmp/rusty-crew-live-cert-4565/evidence-summary.json`
- Visible transcript:
  `/tmp/rusty-crew-live-cert-4565/visible-transcript.txt`

Rendered behavior observed:

- Rusty View connection status was `connected`.
- The selected profile was `tester`.
- The selected session was `tester-session`.
- The UI rendered two reasoning blocks.
- The UI rendered a `read_file` tool block with status `COMPLETED`.
- The final assistant text included `rusty-crew` and `uirustcutover`.
- The inspector event list advanced through many `assistant_text_delta` events
  and ended at cursor `tester-session:490`.
- The debug snapshot recorded `rawEventCount: 230`, `messageCount: 3`, and a
  completed assistant message with block kinds
  `reasoning`, `tool_call`, `reasoning`, `text`.

## Broker Findings

The Playwright broker was attempted first. Those runs did not certify the debug
backend because the rendered Rusty View app talked to the live service on port
`9347` unless the browser URL included `?api=http://127.0.0.1:9348`.

Recorded failed broker run indexes:

- `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260707T120513.283166658Z-354923/run-index.json`
- `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260707T120546.068497439Z-357642/run-index.json`
- `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260707T120607.466419311Z-359741/run-index.json`

The first broker run also exposed a concurrent profile-create config write race:
one isolated-profile setup path failed while renaming a temporary
`service.json` file.

Follow-ups:

- Rusty View #4601: pass `RV_LIVE_BACKEND_URL` into the rendered app and record
  the effective backend in live evidence.
- Rusty Crew #4602: serialize or otherwise harden profile-create/config writes
  under concurrent live-test setup.

## Scope Notes

The asha-planner Den-doc/tool-access prompt was not rerun as part of this
certification. That prompt is still valuable, but it tests MCP profile/tool
configuration and Den document access in addition to the Rust pi-agent brain.
This task certified the Rust pi-agent live provider path, neutral tool
execution, failure recovery, and Rusty View rendering with the disposable
`tester` profile. The asha-planner prompt should stay in its dedicated
MCP/tool-access validation stream rather than being used as the cutover gate.
