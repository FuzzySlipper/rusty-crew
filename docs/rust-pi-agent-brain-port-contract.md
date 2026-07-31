# Rust Pi-Agent Brain Port Contract

Status: implemented cutover record for tasks 4556-4564

This note pins the boundary and parity target used to port the TypeScript
pi-agent brain behavior to Rust. It is intentionally narrower than older
pi-crew audit prose: Rusty Crew did not port the full `pi-ai` provider matrix
or the whole upstream `Agent` runtime. It ported the behavior Rusty Crew uses:
a fresh per-wake agent loop over OpenAI-compatible model surfaces, neutral tool
execution, and the existing `BrainWakeStreamItem` vocabulary.

## Source Grounding

The former TypeScript behavior was in:

- `ts/packages/brain-island/smokes/support/legacy-pi-agent-test-harness.ts`
  (private smoke harness, not a production module export)
- `ts/packages/brain-island/smokes/support/legacy-pi-tool-adapter-test-harness.ts`
  (legacy harness adapter)
- `ts/packages/brain-island/src/brain-module.ts`

The production implementation is in:

- `crates/brains/pi-agent`
- `ts/packages/brain-island/src/brain-module.ts`
- the native bridge pi-agent wake surface

Boundary references:

- `docs/brain-wake-stream-protocol.md`
- `docs/rust-brain-crate-firewall.md`
- `docs/adr/0021-first-class-brain-modules.md`
- `docs/pi-agent-rust-port-inspiration.md`
- `docs/roleplay-boundary-and-rust-migration-plan.md`
- `docs/pi-crew-core-bridge-manifest.md`

Where older docs say that all LLM/provider calls remain TypeScript-owned, read
that as historical unless a newer ADR says otherwise. ADR 0021 and this task
make the brain module boundary language-neutral: Rust may own provider request
construction, provider stream parsing, agent-loop state, and neutral stream
mapping when the module stays behind the approved wake contract.

## Boundary

The Rust pi-agent brain should live under `crates/brains/`, most likely as a
new sibling of `crates/brains/openai-responses`. It may depend only on the
approved Rust brain surfaces:

- `rusty-crew-core-protocol`
- `rusty-crew-core-bridge-api`

It must not depend on coordination internals, persistence, service-host,
adapter crates, local service config, or native bridge implementation crates.

Rust owns for this brain:

- OpenAI-compatible chat-completions request construction.
- Provider SSE parsing and provider error mapping.
- Minimal agent loop: prompt, stream, execute tool calls, repeat until no tool
  calls or terminal failure/guard.
- Event mapping into `BrainWakeStreamItem`.
- Stateful literal `<think>...</think>` scanning.
- Model/API selection logic in a Rust helper with the same reviewed behavior as
  the retired TypeScript den-router helper.
- Typed failure summaries that surface as `wake_failed` or visible
  `provider_status` events according to the stream protocol.

TypeScript remains the transition owner for:

- Profile loading, role/profile assembly, and runtime config expansion.
- Tool selection from profile/toolset config.
- Tool implementation execution, via the generalized neutral buffered tool
  bridge.
- MCP clients and platform adapters.
- Provider-request/tool-call debug store projection at the TS/native boundary.
- Roleplay narrator orchestration for the first cutover.

## Non-Goals

- Do not port Anthropic, Bedrock, Google, local provider SDKs, or the generated
  `pi-ai` provider/model catalog.
- Do not vendor or depend on `/home/research/pi_agent_rust`.
- Do not duplicate Rust coordination decisions inside the brain crate.
- Do not let Rust brain modules bypass profile tool selection or local tool
  profile policy.
- Do not keep a hidden TypeScript pi-agent fallback after cutover unless a
  follow-up explicitly documents a temporary known limitation.

## Cutover Strategy

1. Build the Rust chat-completions streaming client with fake-client tests.
2. Port deterministic event mapping and the stateful reasoning scanner.
3. Port den-router model selection with fake-router tests.
4. Generalize the existing buffered neutral tool execution path so both Rust
   brains use one registry shape instead of a responses-specific one.
5. Implement the Rust pi-agent loop with neutral tool rounds and repeated-call
   guards.
6. Add bridge/native/module wiring so profiles can select the Rust pi-agent
   brain through the normal brain module registry.
7. Switch `piAgentCoreBrainModule` default strategy to the Rust pi-agent brain.
8. Keep `roleplay_narrator` as a TypeScript executor over Rust-owned narrator
   FSM plans. TypeScript invokes Rust pi-agent sub-wakes and tools;
   `roleplay-core` owns phase order, instructions, allowed tools, mandatory
   prelude planning, auto-capture planning, and review decisions.
9. Delete the TypeScript pi-agent internals and drop `@earendil-works/pi-*`
   runtime dependencies after deterministic and live certification. Completed
   by task 4564, except for private smoke harness helpers retained to test
   neutral event/tool mapping without upstream packages.

## Parity Matrix

| Concern | Current behavior | Rust target |
| --- | --- | --- |
| Wake input | `createPiAgentBrain` receives `BrainWakeInput`; builds one fresh `Agent` per wake. | Preserve fresh per-wake loop. Use frozen `BodyState`, assembled prompt, role assembly, selected tool descriptors, and optional provider state inputs from the neutral contract. |
| Prompt assembly | System prompt is `input.systemPrompt` plus `roleAssembly.instructions`; initial messages are role assembly initial messages; pending messages are prompted as user text. | Preserve visible prompt ordering. Request/debug cache should expose the assembled prompt and model/provider options through the same safe debug path. |
| Agent lifecycle events | `agent_start` maps to `started`; `agent_end` maps to `finished`. | Preserve `started` and `finished` event semantics. A stream must terminate with exactly one `actions` or `wake_failed` item. |
| Text deltas | `message_update` with `text_delta` maps non-empty deltas to `text_delta`, except literal think blocks are split. | Preserve non-empty visible text emission. Empty deltas are ignored. |
| Final message fallback | On `message_end`, if no text delta was seen, final assistant text is emitted; if text deltas were seen, final message text is ignored to avoid duplicate output. | Preserve duplicate suppression. Tests must cover final-only text and streamed-plus-final text. |
| Native pi thinking | `thinking_delta` maps to `reasoning_delta` with `format: "pi-thinking"`. Final assistant `thinking` content is emitted on final-message fallback. | Preserve the distinct reasoning stream. If provider exposes chat-completions reasoning deltas, map them to `format: "pi-thinking"` or a documented provider-specific format. |
| Literal `<think>` blocks | Current `splitLiteralThinkBlocks` is per-delta. It separates complete tags inside one delta but can leak tags split across chunk boundaries. | Intentionally improve with a stateful scanner across chunks. Do not leak literal `<think>` tags as assistant text. Unterminated think content remains reasoning until stream end, then closes by stream termination rather than synthetic visible text. |
| Tool selection | TS resolves tools through `resolveToolSession`; selected `BrainTool`s are adapted into pi `AgentTool`s. | Preserve TS-owned selection. Rust receives neutral tool descriptors and uses the shared buffered neutral tool executor. |
| Tool call start | pi `tool_execution_start` starts a debug record and emits `tool_call_started` with local metadata/debug detail id. | Preserve `tool_call_started` vocabulary. Tool call id should remain available through metadata/debug detail where the current neutral event shape permits it; do not invent a second event family. |
| Tool call updates | pi `tool_execution_update` is ignored for stream events, though debug store records partials inside the adapter. | Preserve unless a later task deliberately adds neutral update events. Partial debug updates remain debug-store detail, not chat stream clutter. |
| Tool call finish | pi `tool_execution_end` emits `tool_call_finished` with `isError` and the same debug detail reference. | Preserve event shape and debug detail continuity. Ordinary tool failures are delivered to the model as error tool results. Repeated failures add bounded provider-facing recovery guidance but do not terminate the wake; the model must remain able to correct the call, choose an alternative, or report the problem. |
| Tool result format | `BrainToolResult` text/image content is converted to neutral provider output. A typed `turnDisposition` distinguishes `complete_turn` from `suspend_external`. | Preserve text/image result support. Never collapse successful completion and external deferral into one boolean: an intentional native stop must settle according to its exact disposition and must not become a provider or logical-turn failure. |
| Agent loop | Upstream Agent owns prompt, stream, tool execution, idle wait, and queue clearing. Rusty Crew creates a fresh Agent each wake, then `clearAllQueues`. | Replace with Rust loop: stream provider events, gather tool calls, execute neutral tools, submit tool outputs, repeat. No queue-clearing concept should be needed beyond dropping per-wake state. |
| Repeated tool calls | Currently mostly upstream Agent behavior; responses brain has explicit repeated-call ceiling. | Keep provider continuation-round ceilings distinct from tool-result classification. An ordinary failed tool result never ends the wake merely because the same tool or reason occurred earlier. Exact provider-call loop guards may still fail visibly when their own documented ceiling is exhausted. |
| Provider errors | Final assistant message with `stopReason: "error"` and `errorMessage` maps to visible `text_delta` prefixed with `LLM error:`. | Preserve user-visible failure summaries, but prefer terminal `wake_failed` for provider failures that abort before meaningful model output. Include typed provider status where useful. |
| Usage | The retired TypeScript pi-agent harness did not project usage into neutral stream events. | No required parity projection. If chat-completions usage is available, expose it through transport metrics/debug samples or a future neutral usage event only after contract work. |
| Den-router model selection | Default base URL `http://127.0.0.1:18082`; probe `/v1/models` and `/routes`; choose requested model, else `deepseek-flash`, `grok`, `glm`, `local-coder`, else first model; codex-oauth backend implies responses API unless explicitly configured. Only `openai-responses` and `openai-completions` are accepted. | Preserve exact candidate/default behavior unless a later task deliberately changes it. Unsupported API/provider cases fail clearly; no silent fallback to another brain or provider matrix. |
| Responses protocol | Current den-router factory may select `openai-responses` for codex-backed models even though this port is primarily chat-completions. | Do not build a fake responses shim inside the pi-agent brain. Prefer routing responses providers to the existing Rust `openai-responses` brain, or explicitly document a small compatibility bridge if profile semantics require it. |
| Debug store | TS records a provider request debug snapshot with boundary `pi_agent_options`; tool debug records start/update/finish/fail around TS tool execution. | Preserve operator-visible debug snapshots. For Rust client internals, record request samples at the TS/native boundary as done by `openai-responses`, without leaking secrets. |
| Live event submission | If `submitEvent` is configured, pi-agent brain submits events as they arrive and returns an empty local event list. | Preserve streaming-first behavior. Rust bridge draining must expose events before the wake completes so Rusty View can update live. |
| Mid-turn snapshot policy | Current smokes prove frozen pending messages and body-owned next-wake queue behavior; pi-agent sees only the wake snapshot. | Preserve. The Rust brain must not reach into coordination state mid-turn. Tool calls may execute through the bridge, but new body events wait for next wake unless a future policy changes this. |
| Roleplay narrator | `roleplay_narrator` strategy composes multiple pi-agent turns through a TS executor over Rust FSM plans. | Current boundary: Rust `roleplay-core` owns deterministic narrator sequencing and instructions; TS only executes phase wakes/tools and projects events. Live certification follows in #4607. |

## Review Gates For Implementation Tasks

Deterministic gates to preserve or port:

- `npm run smoke:pi-agent-brain-events -w @rusty-crew/brain-island`
- `npm run smoke:mid-turn -w @rusty-crew/brain-island`
- `npm run smoke:brain-catalog -w @rusty-crew/brain-island`
- `npm run smoke:openai-responses-tool-bridge -w @rusty-crew/brain-island`
- `npm run smoke:rusty-view-chat-read-api -w @rusty-crew/service-host`
- `npm run smoke:rusty-view-chat-contract -w @rusty-crew/brain-island`
- `npm run smoke:roleplay-narrator-brain -w @rusty-crew/brain-island`
- `npm run smoke:roleplay-browser-api -w @rusty-crew/brain-island`
- `npm run smoke:architecture-boundaries`
- `cargo test --workspace`
- `npm run smoke:bridge-contract-parity`
- `npm run smoke:bridge-native-surface`
- `npm run smoke:bridge-fixture-drift`
- `npm run smoke:bridge-fingerprint-drift`
- `npm run smoke:bridge-validation`

Live certification gate before cutover is deliverable:

- Use the debug Rusty Crew service and Rusty View.
- Run one no-tool turn, one model-visible tool turn, and one failure/recovery
  case where practical.
- Confirm visible assistant text, reasoning blocks, tool start/finish/error
  events, final completion, and stream/readback behavior without manual
  refresh.
- Re-run an asha-planner-style Den-doc/tool-access prompt when Den tools are
  healthy enough, or record why it was skipped.

## Documentation Reversals To Apply During Cutover

The implementation series should update older prose as behavior lands:

- `docs/pi-crew-core-bridge-manifest.md` still contains a historical "LLM
  boundary" section that assumed TS owns all provider streaming.
- `docs/roleplay-boundary-and-rust-migration-plan.md` says not to put model
  provider SDK calls in Rust merely because roleplay uses them. That remains a
  good roleplay-local warning, but the Rust pi-agent port is a brain-boundary
  decision, not a roleplay route decision.
- `docs/pi-package-source-lock.md`, README surfaces, and package manifests
  should stop treating `@earendil-works/pi-agent-core` / `pi-ai` as active
  runtime dependencies after the TypeScript internals are deleted.

## Open Review Points

- Whether profiles that currently specify `api: "openai-responses"` under the
  `pi-agent-core` module should be migrated automatically to the
  `openai-responses` module or rejected with a clear config error during the
  clean cutover.
- Whether `BrainEvent::ToolCallStarted` and `ToolCallFinished` should grow an
  explicit neutral `tool_call_id` field or continue projecting call identity
  only through metadata/debug details. This should be handled as contract work,
  not ad hoc in the Rust pi-agent crate.
- Whether usage should become a first-class neutral event/metric for both Rust
  brains. Current pi-agent parity does not require it.
