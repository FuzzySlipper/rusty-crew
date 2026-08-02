# DeepSeek Responses Live Certification

Task `#6530` certified the native Rust Responses brain against DeepSeek's direct
API on the SQLite-backed debug service at `127.0.0.1:9348`. The credential was
read from the service-owned provider secret record and was not copied into repo
files or evidence.

## Certified Provider

| Field | Value |
| --- | --- |
| Alias | `deepseek-flash-responses` |
| Protocol | `responses` |
| Dialect | `deepseek` |
| Provider kind | `deepseek` |
| Model | `deepseek-v4-flash` |
| Base URL | `https://api.deepseek.com` |
| Credential | database-backed `api_key`, redacted in readback and snapshots |

The provider contract follows DeepSeek's
[Responses API guide](https://api-docs.deepseek.com/guides/responses_api/).

## Live Scenarios

The live smoke created a disposable full coding agent and verified:

- basic text and visible reasoning streaming;
- stateless multi-turn continuity with prior provider output ordered before the
  new user message;
- sequential tool rounds;
- two function calls emitted in one parallel provider response;
- a real `tool_call_failed` event followed by a successful recovery call;
- a long agentic inventory with eight successful tools;
- exact early request snapshots with unsupported OpenAI stateful/cache fields
  absent;
- DeepSeek semantic SSE event names and usage accounting;
- automatic context-cache hits reported through `cachedInputTokens`;
- continuation hydration and completion across an intentional service restart.

The live implementation work also exposed and fixed protocol issues that
synthetic OpenAI fixtures did not reveal: DeepSeek reasoning input uses a
content-part sequence, stateless replay must retain the full ordered context,
provider item identities must be de-duplicated during replay, streamed
reasoning may need to be synthesized into replay state, and parallel tool
continuation must send reasoning plus all adjacent function calls before any
function output.

## Evidence

Normal provider certificate:

- evidence root:
  `/home/system/rusty-crew-debug/evidence/task-6530/msbt59jr`
- six completed provider-backed wakes
- long wake usage: `118634` input, `98432` cached input, `1633` output,
  `804` reasoning-output tokens
- streamed `response.function_call_arguments.delta` and
  `response.function_call_arguments.done` events were assembled into canonical
  calls before execution; the run completed three sequential calls, two
  parallel calls, recovery, and an eight-tool long turn

Restart-safe certificate:

- evidence root:
  `/home/system/rusty-crew-debug/evidence/task-6530/msbqcdd0`
- debug-only work quantum temporarily set to one continuation round
- six wake epochs spanned the intentional restart scenario
- four tools completed after durable continuation hydration
- debug service configuration restored to 64 rounds after certification

Rusty View browser certificates:

- activity/tool projection broker packet:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T115434.839718446Z-3546543/run-index.json`
- reasoning controls broker packet:
  `/home/agent/.cache/den-playwright/runs/rusty-view/rusty-view-20260802T115559.990026303Z-3552419/run-index.json`
- Chromium rendered DeepSeek reasoning and real tool activity inside completed
  assistant turns; the reasoning control visibly expanded and collapsed the
  provider reasoning content
- both broker runs passed, and their final screenshots and transcript evidence
  were inspected manually

The certifier deletes disposable profiles on success. Exact prompt-debug
snapshots are intentionally bounded: early requests remain fully inspectable;
large tool-heavy requests retain a hash and preview after the 80 KB cache cap.
The in-memory debug cache is not expected to survive the restart proof, while
chat events, logical-turn state, provider state, and evidence files are durable.

Run the normal certificate with:

```bash
npm run smoke:deepseek-responses-live-debug-service -w @rusty-crew/brain-island
```

The restart variant additionally requires the debug service work quantum to be
one and is enabled with:

```bash
RUSTY_CREW_DEEPSEEK_RESPONSES_RESTART_PROOF=1 \
  npm run smoke:deepseek-responses-live-debug-service -w @rusty-crew/brain-island
```
