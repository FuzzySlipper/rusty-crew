# Mid-Turn Context Compaction

Rusty Crew compacts a brain's model-facing continuation projection when a
healthy logical turn accumulates enough provider and tool exchanges to approach
the configured context threshold. Compaction does not end the logical turn,
create a synthetic user message, or impose a turn lifetime.

## Authority Boundary

`rusty-crew-brain-runtime` owns the shared policy validation, usage decision,
provider context-limit classification, and artifact contract. The Chat
Completions and OpenAI Responses brain crates own their protocol-specific
projection replacements.

TypeScript transports the profile context policy and provider context-window
size into the native brain configuration. It also projects neutral lifecycle
events into chat SSE. It does not decide when to compact, mutate continuation
state, or provide a fallback compaction loop.

The production policy reuses profile context configuration:

- `strategyId: rolling_summary_compaction`
- `autoCompactionEnabled`
- `compactAtPercent`
- `targetPercentAfterCompaction`
- the selected provider's `contextWindowTokens`

Provider-reported input usage is authoritative when available. Otherwise Rust
uses a conservative estimate of the serialized model projection.

## Safe Boundary

Compaction is evaluated only between completed provider/tool rounds, before the
next provider request. An unresolved call is never split from its result. The
replacement projection retains:

- the frozen request and profile/system instructions;
- unresolved calls and pending outputs;
- recent complete provider/tool exchanges;
- no-progress evidence and output-continuation overlap state;
- protocol-specific provider invariants.

The raw transcript, complete tool telemetry, debug request samples, and durable
provider history remain unchanged. Only the next model request projection is
smaller.

## Chat Completions

The Chat Completions brain retains the initial instruction/request prefix and a
bounded recent suffix of complete exchanges. Older completed exchanges become
a deterministic system summary in the model-facing `messages` projection.
`durableMessages` retains the full history.

Each replacement records a `BrainContextCompactionArtifact` and checkpoints the
replacement before another provider request. A resumed process therefore uses
the same compacted projection and cannot execute an already completed tool call
again.

## OpenAI Responses

The Responses brain retains frozen base history and recent complete
function-call/output pairs. It records the compacted evidence as additional
instructions rather than fabricating a user message.

Previous-response chaining is deliberately invalidated after compaction. The
artifact records `providerChainAction: rebuild_replay_after_compaction`, and the
next request rebuilds explicit replay input from the compacted projection. This
is preferable to claiming that a provider-side response chain was compacted
when its hidden state was not.

## Checkpoint And Provenance

Mid-turn compaction artifacts are embedded in the opaque brain continuation
payload alongside the exact replacement projection. Rust coordination persists
that payload as one continuation checkpoint in SQLite or PostgreSQL. This is
the restart authority: an artifact cannot be committed without the projection
it describes, nor can the projection be resumed without its artifact history.

Session-level context-compaction artifact storage remains useful for later
cross-turn summary strategies and admin readback. It is not used as a second
mid-turn lifecycle authority.

## Diagnostics And Failure

The brain emits neutral provider-status metadata with one of these kinds:

- `context_compaction_started`
- `context_compaction_completed`
- `context_compaction_failed`

The service projects those as dedicated chat/SSE events. They remain outside
model context unless a future strategy deliberately assembles them.

If a replacement cannot be formed, or a provider rejects the context before a
replacement can complete, the logical turn checkpoints and pauses for
retry/cancel attention. It does not `WakeFailed` or retry-loop.

## Strategy Evolution

The shared decision and artifact types are independent of the projection
algorithm. Deterministic pruning is the first active implementation. A later
model-produced summary or protocol-native compaction operation can implement a
different strategy behind the same Rust-owned lifecycle and checkpoint
contract.
