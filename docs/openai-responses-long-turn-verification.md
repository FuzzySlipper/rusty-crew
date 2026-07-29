# OpenAI Responses long-turn verification

Rusty Crew's native Responses loop durably yields after 64 continuation rounds
by default. Operators can set a different positive service-wide work quantum
with `RUSTY_CREW_OPENAI_RESPONSES_WORK_QUANTUM_CONTINUATION_ROUNDS`. This is a
scheduling quantum, not a logical-turn limit. Explicit cancellation remains the
operator-control boundary.

Provider request deadlines remain disabled by default. When explicitly set,
`RUSTY_CREW_OPENAI_RESPONSES_PROVIDER_REQUEST_TIMEOUT_MS` applies to each HTTP
request independently of the continuation work quantum. It never measures
total logical-turn lifetime.

Terminal provenance uses distinct reason codes:

- `provider_request_timeout`: configured provider HTTP request deadline;
- `provider_response_failed` or `provider_response_incomplete`: provider
  terminal rejection;
- `provider_request_cancelled`: explicit cancellation.

The admin diagnostics response reports the effective
`workQuantumContinuationRounds`, provider request timeout mode, and retained
Responses wake metrics. Failed metrics include `terminalFailureReasonCode` and
`terminalFailureSource`.

## Verification

Run the deterministic 20-tool replay regression:

```bash
cargo test -p rusty-crew-openai-responses-brain \
  long_multi_tool_replay_finishes_with_reasoning_and_output_policy_intact
```

Build the native addon, restart only the debug service, and run the live hard
fixture with the debug provider alias `gpt-5.6-luna`:

```bash
npm run build:native
systemctl --user restart rusty-crew-debug.service
cd /home/dev/goblinbench
python3 scripts/gb-run.py \
  --scenario coding.leased-dag-queue-rust \
  --candidates candidates.rusty-crew-native-gpt56-medium.json \
  --candidate rusty-crew-native-gpt-5-6-luna-reasoning-medium
```

The candidate configuration must retain `http://127.0.0.1:9348`,
`rusty-crew-debug.service`, `provider_protocol: responses`, and an explicit
session reasoning effort. Never point this certification at port 9347 or
`rusty-crew.service`.

Task 6367 was certified on the debug service with the work quantum temporarily
set to `1`, provider `responses-proxy-cert-5389`, and session
`chat-cert-ms61f7kh-session`. The one logical turn recorded the lifecycle
`admitted -> continuation_claimed -> continuation_yielded ->
continuation_claimed -> completed` under one source wake. Its yielded
checkpoint used `openai-responses-continuation-v1`, retained the completed tool
round, and resumed to one terminal assistant message without duplicating tool
calls. The debug service was restored to the default quantum `64` afterward.

Task 5963 was reproduced on the debug service with run
`run-20260718-022414-c785c55b`: 13 successful local tool calls and nine
provider requests ended in the old hard-coded eight-continuation guard. The
guard incorrectly emitted `provider request timeout`; no provider request
deadline was configured. Provider-state absence was present only on the first
wake and did not cause the failure.
