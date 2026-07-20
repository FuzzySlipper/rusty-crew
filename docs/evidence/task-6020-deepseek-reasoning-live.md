# Task 6020 DeepSeek Reasoning Live Evidence

This certificate targets only `rusty-crew-debug.service` on port `9348`. It
uses the configured `deepseek-flash` provider and never contacts the live
service on port `9347`.

The smoke temporarily applies the explicit `deepseek` dialect with
`thinkingMode: enabled` and `reasoningHistory: tool_calls_only`, creates a
disposable full-agent profile, and asks the live model to select `git_status`
and `read_file` in two sequential tool rounds. It then sends a second user
turn and checks the exact provider-request debug snapshot:

- every assistant tool-call message retains non-empty `reasoning_content`;
- the same tool-call reasoning is replayed byte-for-byte after restart-safe
  provider-state hydration on the second wake;
- non-tool assistant messages carry no historical `reasoning_content`;
- ordinary visible messages, tool calls, and tool results remain in order.

The profile is hard-deleted and the original provider revision/configuration
is restored in `finally`. Evidence records only SHA-256 digests, never raw
reasoning text.

```bash
npm run smoke:chat-completions-deepseek-live-debug-service -w @rusty-crew/brain-island
```

## 2026-07-19 Run

- Provider alias/model: `deepseek-flash`
- First-wake provider requests: `3`
- Sequential tools: `git_status`, then `read_file`
- Second-wake provider requests: `1`
- First tool-call reasoning SHA-256:
  `2baa1ffcf58063b14f3effc1791c79cc2d159c6781ceacb52e2d220a320350f1`
- Second-wake restored SHA-256: identical
- Raw reasoning persisted in evidence: `false`
- Provider restoration: `standard`, `provider_default`, `provider_default`
