# Agentic Sessions: Epoch-Based Execution with a Writable Continuity Graph

## Status

Design sketch for a DSH/Cordis prototype.

## Core Thesis

Long-running agent work should not be modeled as a chat transcript that happens to contain tool calls.

A chat transcript conflates four different things:

1. Historical record of what happened.
2. Current working state of the task.
3. Context supplied to the model.
4. Communication between the agent, user, and teammates.

That model fits conversation. It fits multi-hour software work poorly.

Instead:

* The **job** is durable.
* A model context is a temporary **epoch** leased to work on that job.
* The transcript is an immutable **event journal**, not the agent's memory.
* Current task understanding lives in a mutable, versioned **continuity graph**.
* Each epoch begins with a compiled **preparation packet**.
* Each epoch continuously prepares a handoff for its **successor**.
* User and teammate messages arrive through asynchronous mailboxes rather than defining turn boundaries.
* Compaction becomes succession to a new epoch rather than pretending one context remained continuous.

The transcript becomes a flight recorder. The continuity graph becomes working memory.

---

## Goals

1. Preserve provider prefix-cache efficiency during long work.
2. Stop replaying an increasingly polluted transcript as if it were current state.
3. Give the agent an erasable foreground without deleting forensic history.
4. Maintain a usable handoff continuously rather than synthesizing one near context exhaustion.
5. Make inherited state explicitly trust-but-verify.
6. Support asynchronous user, teammate, and system communication.
7. Give graceful shutdown, provider preemption, and crash recovery first-class semantics.
8. Fit DSH as a modular Cordis composition rather than requiring a harness rewrite.
9. Support multi-agent work without allowing concurrent semantic overwrites.
10. Keep the user-facing interface conversational even when the execution model is not.

## Non-Goals

* Claiming persistent personal identity across model contexts.
* Reconstructing hidden model activations.
* Replacing repository state, task databases, message boards, or durable documentation.
* Treating the continuity graph as automatically true.
* Loading the entire continuity graph into every model call.
* Requiring a model call for every state update.
* Preserving every exploratory thought in the active foreground.

---

# 1. Execution Model

## 1.1 Durable Objects

### Job

The durable user objective.

A job may span:

* Many model calls.
* Many context compactions.
* Several providers or models.
* Human interruptions.
* Multiple agents.
* Process restarts.
* Hours or days of work.

```text
Job
  id
  workspace
  root goal
  acceptance criteria
  constraints
  current phase
  active epoch
  continuity graph revision
  event journal position
  status
```

### Epoch

One context-bearing worker instance.

An epoch starts with a preparation packet and ends with a sealed handoff, crash, or hard termination.

An epoch may contain hundreds of model and tool steps. It should normally be long enough that its initial cache miss is economically negligible relative to useful work performed afterward.

```text
Epoch
  id
  job id
  predecessor epoch
  provider / model / reasoning effort
  preparation packet hash
  graph base revision
  start event position
  status
  budget and lease
  final seal
```

### Step

One model output, tool call, tool result, or internal control action inside an epoch.

Steps remain append-only within the epoch.

### Event

An asynchronous occurrence relevant to the job:

* User message.
* Teammate message.
* Tool completion.
* Provider warning.
* Cost or token threshold warning.
* Soft cancellation.
* Hard cancellation.
* File or repository event.
* Epoch start.
* Continuity patch.
* Seal.
* Crash.

Events belong to the durable journal. Only selected events enter the current model context.

### Seal

The certified end state of an epoch.

A seal records:

* Continuity graph revision.
* Event-journal cursor.
* Repository and workspace state.
* Pending inbox and outbox positions.
* Open tool operations.
* Inherited claims not rechecked.
* Current uncertainty.
* Recommended next action.
* Reason the epoch ended.

### Successor

The next epoch that receives a compiled preparation packet.

The successor inherits recorded state, not autobiographical memory.

---

## 1.2 Epoch Lifecycle

```text
PREPARING
  Compile preparation packet.
  Resolve model route and tools.
  Snapshot job and graph revisions.

RUNNING
  Perform ordinary model and tool steps.
  Update the continuity graph through sideband deltas.
  Read and send asynchronous messages.
  Preserve append-only context and prefix caching.

DRAINING
  Soft stop or preemption requested.
  Finish the current atomic operation.
  Begin no substantial new branch.
  Reconcile pending work.

CERTIFYING
  Audit active graph topics.
  Mark inherited claims not personally rechecked.
  Record repository, task, inbox, and outbox state.
  Produce a seal.

SEALED
  Safe to stop, compact, migrate, or start a successor.

CRASHED
  Epoch ended without certification.
  A rescue epoch must reconcile the unsealed event tail.
```

A user message does not automatically end an epoch.

An agent message does not automatically end an epoch.

Compaction, provider migration, a major phase transition, graceful cancellation, or context exhaustion normally does.

---

# 2. Four State Surfaces

## 2.1 Epoch Scratch Stream

The active model context.

It contains:

* Preparation packet.
* Current epoch's reasoning and summaries.
* Tool calls and results.
* Notifications.
* Continuity deltas emitted during the epoch.
* Recent communication.

Properties:

* Append-only.
* Cache-friendly.
* Temporary.
* Allowed to contain exploration and discarded hypotheses.
* Never treated as the authoritative current account after the epoch ends.

The harness should not rebuild this context whenever the continuity graph changes. The current worker already knows the change it just made.

## 2.2 Immutable Event Journal

The complete forensic record.

It contains every material event in sequence:

```text
event id
job id
epoch id
monotonic sequence
event type
timestamp
actor
payload or payload reference
causal parent
delivery state
```

The journal supports:

* Audit.
* Replay.
* Rescue.
* Debugging.
* Retrieval of discarded evidence.
* Reconstruction of continuity changes.
* Usage and cost analysis.

It is not normally inserted wholesale into model context.

## 2.3 Mutable Continuity Graph

The current, hypertextual working account of the job.

It consists of addressable topic nodes rather than one growing Markdown summary.

Example topic IDs:

```text
goal/current
architecture/upstream-contract
architecture/downstream-shape
decision/remove-local-scheduler
hypothesis/event-loop-race
verification/integration-suite
risk/legacy-plugin-assumption
next/rewrite-adapter-layer
message/user-compatibility-question
```

A topic node should support:

```yaml
id: architecture/lifecycle-ownership
kind: current-model
title: Lifecycle ownership moved upstream
state: active
epistemic: verified
body: >
  The upstream system now owns lifecycle scheduling.
  The downstream repository should express policy and configuration
  rather than duplicate lifecycle machinery.
evidence:
  - commit:upstream@84cd13
  - test:integration-lifecycle-17
  - report:scout-epoch-4
relations:
  supersedes:
    - architecture/local-scheduler
  supports:
    - decision/remove-local-scheduler
  blocks:
    - next/rewrite-adapter-layer
stale_if:
  - upstream lifecycle API changes
owner: captain
created_epoch: epoch-4
last_reviewed_epoch: epoch-7
review_state: rechecked-this-epoch
revision: 12
```

The graph must support:

* Add topic.
* Replace topic body.
* Mark active, dormant, resolved, superseded, contradicted, or archived.
* Add or remove links.
* Attach evidence.
* Change epistemic status.
* Record staleness conditions.
* Mark inheritance and review state.
* Merge a worker proposal.
* Restore an earlier revision.
* Select topics for the next preparation packet.

Removing a topic from the foreground must archive or supersede it, not erase its history.

**Erase from the foreground, never from the record.**

## 2.4 Compiled Preparation Packet

A deterministic view over the graph, journal, mailbox, and workspace.

The packet should contain only what the successor needs to resume competently:

```text
JOB
  Goal
  Acceptance criteria
  Constraints

CURRENT PHASE
  What is being attempted now
  Why this phase exists

CURRENT MODEL
  Present architectural or causal understanding

INVARIANTS
  Facts and constraints that must remain true

DECISIONS
  Settled choices and their evidence

OPEN QUESTIONS
  Unknowns, assumptions, and blockers

VERIFICATION STATE
  What has been checked
  What remains unverified
  What was inherited without rechecking

NEXT ACTIONS
  Concrete executable continuation

INBOX
  Unread or pending messages

DORMANT TOPIC INDEX
  IDs and one-line descriptions for retrievable material

PREDECESSOR SEAL
  Why the previous epoch ended
  Known ambiguity or recovery notes
```

The packet is not the whole graph.

A large graph is acceptable. A large packet is a failure of selection.

---

# 3. Identity and Trust Semantics

The harness should define identity operationally:

* **I**: the currently executing epoch.
* **Predecessor**: the epoch or worker that produced inherited state.
* **Successor**: the future worker receiving the outgoing handoff.
* **The job**: the durable objective spanning all epochs.

Suggested preparation-packet language:

> You are the current worker epoch for this job. The incoming continuity graph was produced by one or more predecessors. It is recorded testimony and state, not your personal recollection. Trust entries according to provenance, verification status, and staleness conditions. Recheck high-impact assumptions when appropriate. Your successor will not inherit your unstated priors or unrecorded reasoning, so maintain the continuity graph as you work.

This avoids the disruptive pattern:

> Why did I do this earlier?

It replaces it with:

> A predecessor recorded this decision. What evidence supports it, and does it still hold?

The distinction is functional and does not require a philosophical position on model identity.

---

# 4. Epistemic Status

Avoid free-floating numeric confidence such as `0.83`.

Use categorical states tied to evidence:

* **Verified**: directly checked with a named verifier and explicit scope.
* **Supported**: substantial evidence exists, but decisive verification is incomplete.
* **Inferred**: derived from stated evidence.
* **Assumed**: provisional premise used to continue.
* **Unknown**: unresolved.
* **Stale**: validity may have expired.
* **Contradicted**: active evidence conflicts.
* **Refuted**: rejected by named evidence.

Separately track review provenance:

* Produced this epoch.
* Verified this epoch.
* Inherited and rechecked.
* Inherited and relied upon.
* Inherited but not rechecked.
* Recovered after crash.
* Proposed by teammate.
* Accepted by captain.
* Awaiting review.

A seal should not claim that every inherited entry was personally reevaluated.

Example:

> Carried forward from epoch 4, where integration test V17 was reported passing. The current epoch did not rerun that test.

The epoch certifies that the handoff accurately represents recorded state, including uncertainty. It does not certify omniscience.

---

# 5. Write-Ahead Handoff

The handoff must be maintained throughout the epoch.

Do not wait until context exhaustion and ask the model to reconstruct everything important from memory.

At any random point, the continuity graph should already be mostly usable by a successor.

The end-of-epoch operation is:

1. Audit the existing graph.
2. Reconcile provisional changes.
3. Mark inherited and unverified material.
4. Record current external state.
5. Seal.

It is not:

1. Remember several hours of work.
2. Write one giant summary.
3. Hope nothing important fell out during compaction.

---

# 6. Continuity Updates Without Tool-Call Overhead

## 6.1 MVP: Explicit Tool

Start with one promoted tool:

```text
continuity_patch
```

Operations:

```text
add_topic
replace_topic
set_state
set_epistemic
attach_evidence
link
unlink
archive
propose_merge
accept_merge
mark_reviewed
```

Use strict revision checks:

```json
{
  "job_id": "job-42",
  "base_revision": 184,
  "ops": [
    {
      "op": "replace_topic",
      "id": "architecture/lifecycle-ownership",
      "body": "The upstream system now owns lifecycle scheduling..."
    },
    {
      "op": "set_epistemic",
      "id": "architecture/lifecycle-ownership",
      "value": "verified",
      "evidence": ["test:integration-lifecycle-17"]
    },
    {
      "op": "set_state",
      "id": "hypothesis/event-loop-race",
      "value": "refuted"
    }
  ]
}
```

Reject stale `base_revision` values and return the new revision.

## 6.2 Mature Form: Sideband Response Channel

Eventually, continuity updates should be a harness-native assistant content type rather than a normal model-facing tool.

One model response can contain three logical channels:

```text
action
  User-visible prose or a tool invocation.

continuity_delta
  Hidden structured changes to durable working state.

outbox
  Optional messages to the user or teammates.
```

The harness should:

1. Parse and validate the sideband.
2. Apply it atomically.
3. Append the sideband to the internal epoch stream.
4. Hide it from the user-facing conversation.
5. Preserve the previous graph revision.
6. Report validation failures to the current epoch.

This avoids an extra inference round trip while preserving the state update in the append-only context.

The wire representation should be deterministic JSON. The model-facing graph projection can remain readable prose.

## 6.3 DeepSeek Bootstrap Constraint

DeepSeek V4 appears sensitive to first-request tool and context composition.

Therefore:

* Do not add the continuity tool to the Anchored Standard bootstrap surface.
* Activate it only after profile promotion.
* Prefer a hidden harness channel over a large family of new tools.
* Keep the model-facing protocol compact.
* Avoid injecting the entire continuity index into the first request.
* Preserve deterministic ordering of any injected sections.

---

# 7. Cache Strategy

## 7.1 Within an Epoch

Maintain ordinary append-only model and tool history.

Do not reconstruct the current context whenever the graph changes.

A continuity patch:

* Updates external durable state.
* Remains visible in the epoch's internal append-only stream.
* Does not cause the preparation packet to be recompiled mid-epoch.

This preserves prefix caching.

## 7.2 Between Epochs

Accept one deliberate cache break.

Compile a fresh, smaller packet from authoritative current state rather than replaying the old transcript.

The reset cost is amortized across the next long epoch.

## 7.3 Prompt Ordering

Order the packet from least volatile to most volatile:

1. Stable system and provider instructions.
2. Stable tool schemas.
3. Durable job identity and constraints.
4. Slowly changing architectural model.
5. Decisions and verification state.
6. Current plan and open questions.
7. Inbox and immediate next action.

Avoid near the prefix:

* Timestamps.
* Random ordering.
* Regenerated stylistic prose.
* Unstable topic IDs.
* Non-deterministic JSON serialization.
* Frequently changing usage counters.

Use byte-stable compilation wherever possible.

---

# 8. Asynchronous Communication

## 8.1 User Inbox

A user message becomes a durable event.

It does not automatically terminate the current epoch.

At a safe seam, the harness may inject:

```text
One user message is waiting.
Priority: normal
Topic: compatibility requirement
Message ID: user-msg-38
```

The agent can:

* Read it immediately.
* Defer it until an atomic operation finishes.
* Acknowledge it.
* Ask for clarification.
* Continue unrelated work.
* Block on it.

## 8.2 User Outbox

The agent can send a message without ending work:

```text
notify_user
request_user_input
report_checkpoint
report_risk
```

A user-facing assistant message is an outbox event, not necessarily the final output of a model turn.

## 8.3 Waiting

The agent should never burn inference polling for a response.

It may declare:

```text
await_event: user-msg-38-response
```

The harness suspends execution deterministically and wakes the epoch when the event arrives.

Alternatively:

```text
blocked_on: user-msg-38-response
continue_with:
  - next/refactor-tests
  - next/remove-dead-adapter
```

The agent continues independent work while the question remains pending.

## 8.4 Message Delivery Boundaries

Notifications should usually enter the active context:

* After a tool result.
* Before starting a new major branch.
* At a task-state transition.
* At an explicit inbox check.
* When priority is urgent.

Do not inject arbitrary user text in the middle of a tool protocol or atomic file mutation.

---

# 9. Cancellation and Preemption

## 9.1 Soft Cancel

A soft cancel means:

1. Stop beginning major new work.
2. Finish the current atomic operation.
3. Reconcile task and repository state.
4. Update the continuity graph.
5. Certify and seal.
6. Stop safely.

## 9.2 Hard Cancel

A hard cancel immediately terminates the epoch.

The epoch becomes `CRASHED`.

A rescue epoch must reconcile the unsealed tail.

## 9.3 Provider and Budget Warnings

The same draining path should handle:

* Context headroom approaching the limit.
* Token or billing budget approaching a threshold.
* Provider maintenance.
* Rate-limit degradation.
* Route migration.
* Machine shutdown.
* Maximum wall-clock lease.
* Repeated provider errors.

Example injected control event:

```text
Epoch lease is approaching expiration.
Finish the current atomic action.
Begin no new substantial branch.
Reconcile and seal the continuity graph.
```

The harness may also mechanically deny new subagents or expensive tools after entering `DRAINING`.

---

# 10. Rescue Epochs

A rescue epoch starts after an unsealed crash.

Input:

* Last valid seal.
* Unsealed event-journal tail.
* Provisional continuity patches.
* Current continuity graph revision.
* Git status.
* Uncommitted diff.
* Files changed after the last seal.
* Current task and team state.
* Incomplete tool operations.
* Pending inbox and outbox messages.
* Provider error information.

Initial rescue instruction:

> Do not continue implementation yet. Reconcile durable state, reconstruct the best available continuity graph, and mark recovered or uncertain material. Produce a rescue seal before resuming ordinary work.

The rescue process should:

1. Determine which actions definitely completed.
2. Detect tool calls without corresponding completion.
3. Compare repository state to the last seal.
4. Apply safe provisional continuity patches.
5. Mark ambiguous entries as recovered or uncertain.
6. Reconstruct next actions.
7. Seal.
8. Start a normal successor epoch.

A good write-ahead graph limits rescue to the unsealed tail rather than the entire historical transcript.

---

# 11. Multi-Agent Continuity

## 11.1 Ownership Model

Each worker receives:

* Its own private epoch scratch stream.
* A private continuity branch.
* A relevant projection of the global graph.

The captain owns the global continuity graph.

Workers submit proposed graph patches with their reports.

The captain may:

* Accept.
* Reject.
* Edit.
* Downgrade epistemic status.
* Request verification.
* Merge several proposals.

## 11.2 Concurrency Control

Use:

* Monotonic graph revisions.
* Per-topic revisions.
* Optimistic concurrency.
* Attempt IDs or ownership capabilities.
* Explicit merge conflicts.

A stale worker must not overwrite a newer shared understanding.

## 11.3 Worker Completion Fold

When a worker completes a task:

1. Full report enters immutable history.
2. Worker proposes topic changes.
3. Captain reviews the proposal.
4. Accepted conclusions enter the global graph.
5. Evidence links point back to the full report.
6. The preparation packet includes only the folded result.

This preserves evidence without flooding the captain with entire worker transcripts.

## 11.4 AgentTeams Integration

AgentTeams already supplies useful lower-level semantics:

* Durable continuable members.
* Dependency-aware tasks.
* Mailboxes.
* Automatic waking and reuse.
* Attempt-scoped ownership.
* Stale-update rejection.
* Safe reassignment.

Continuity adds a semantic layer above task state:

```text
Task graph:
  Who owns what and what depends on what?

Continuity graph:
  What does the team currently believe, why, and what follows from it?
```

The two graphs should remain separate but linked.

---

# 12. Suggested Cordis Composition

Exact hook and service names should be confirmed against the current DSH APIs. The intended modular split is:

## 12.1 `continuity-store`

Responsibilities:

* Persist jobs, epochs, topics, revisions, seals, and journal events.
* Provide atomic graph patches.
* Enforce revision checks.
* Preserve immutable revision history.
* Expose retrieval by topic ID, relation, evidence ID, epoch, or event range.

Possible initial storage:

```text
<workspace>/.dsh-continuity/
  jobs/<job-id>/
    job.json
    graph.json
    topics/
    revisions/
    epochs/
    seals/
    journal.jsonl
    inbox/
    outbox/
```

SQLite is likely preferable once querying and concurrency matter. A file-backed MVP is easier to inspect.

## 12.2 `epoch-manager`

Responsibilities:

* Create and transition epochs.
* Maintain leases and budgets.
* Handle draining and certification.
* Start successor and rescue epochs.
* Bind an active epoch to a DSH session or agent.
* Coordinate compaction boundaries.

## 12.3 `context-compiler`

Responsibilities:

* Select active graph topics.
* Compile deterministic preparation packets.
* Keep serialization byte-stable.
* Build dormant-topic indexes.
* Include unread message notifications.
* Respect token budgets.
* Record exactly which graph revision produced a packet.

Selection inputs:

```text
topic state
priority
current phase
dependency links
recent access
staleness
open blockers
next actions
unread messages
model route
token budget
```

## 12.4 `continuity-tools`

MVP tool surface:

```text
continuity_status
continuity_read
continuity_patch
continuity_search
continuity_seal
continuity_await_event
continuity_send_message
```

Later, replace frequent patch and outbox operations with hidden sideband content types.

## 12.5 `event-mailbox`

Responsibilities:

* Accept user, teammate, provider, and control events.
* Deliver notifications at safe seams.
* Track unread, acknowledged, deferred, and resolved state.
* Suspend and wake epochs without inference polling.
* Separate user-visible messages from epoch termination.

## 12.6 `seal-and-rescue`

Responsibilities:

* Gather repository and task state.
* Detect incomplete operations.
* Produce seal templates.
* Create rescue packets.
* Prevent normal continuation from an unsealed crash until reconciliation completes.

## 12.7 Optional Web Surface

Display:

* Current epoch and status.
* Active graph topics.
* Open questions.
* Epistemic state.
* Unread messages.
* Epoch lineage.
* Last seal.
* Pending soft stop.
* Rescue state.
* Topic revision history.
* Links from folded claims to source evidence.

The UI should make the present state prominent and historical trails inspectable rather than rendering another giant chat log.

---

# 13. DSH Hook Sketch

Potential integration points:

## Request Assembly

Before a new epoch's first model request:

1. Resolve job and epoch.
2. Compile preparation packet.
3. Register packet as a dedicated system or context section.
4. Record packet hash and graph revision.
5. Preserve Anchored Standard bootstrap constraints.

## Assistant Completion

After each assistant output:

1. Parse sideband continuity deltas.
2. Parse outbox messages.
3. Validate revisions.
4. Apply graph patches atomically.
5. Record journal events.
6. Hide sideband from the user view.
7. Return any validation error to the active epoch.

## Tool Boundaries

After each tool completion:

1. Record tool result.
2. Check urgent inbox events.
3. Check soft-stop and provider warnings.
4. Optionally remind the model of pending certification.
5. Avoid rebuilding the preparation packet.

## Compaction Boundary

Instead of ordinary transcript summarization:

1. Send a draining control event.
2. Ask the current epoch to reconcile and seal.
3. Store the old epoch transcript as journal material.
4. Start a successor epoch.
5. Compile a new preparation packet from durable state.
6. Continue under the same job and user-facing session.

## Session or Process Restart

1. Locate active jobs.
2. Find the last valid seal.
3. Detect unsealed event tails.
4. Start either a normal successor or rescue epoch.
5. Rebind mailboxes and team state.

---

# 14. Implementation Phases

## Phase 0: Instrument Existing Sessions

Before changing behavior, record:

* Compaction boundaries.
* Token and context size.
* Provider cache data where available.
* User interruptions.
* Tool counts.
* Model restarts.
* Existing handoff or summary creation.
* Failures caused by lost assumptions.
* Repeated rediscovery of earlier findings.

Goal: establish where transcript continuity currently breaks down.

## Phase 1: File-Backed Continuity MVP

Implement:

* Job record.
* Epoch record.
* Topic graph.
* Immutable revisions.
* `continuity_read`.
* `continuity_patch`.
* `continuity_status`.
* Manual `continuity_seal`.

Continue using the ordinary DSH transcript.

Goal: determine whether agents can maintain useful topic-oriented handoffs.

## Phase 2: Compaction as Succession

Replace standard compaction for selected profiles:

1. Drain.
2. Seal.
3. Start successor.
4. Compile preparation packet.
5. Continue without transcript replay.

Goal: prove that a fresh context can resume real work from graph state.

## Phase 3: Hidden Sideband and Async Mailbox

Add:

* Hidden continuity-delta parsing.
* User inbox.
* Agent outbox.
* Safe-seam delivery.
* `await_event`.
* User messages that do not terminate the epoch.

Goal: remove ordinary tool-call overhead from frequent state and communication operations.

## Phase 4: Soft Stop, Provider Preemption, and Rescue

Add:

* Epoch leases.
* Budget and context warnings.
* `DRAINING` and `CERTIFYING`.
* Hard-stop detection.
* Rescue packet compiler.
* Rescue-first continuation policy.

Goal: make interruption safe rather than exceptional.

## Phase 5: AgentTeams Branching

Add:

* Private worker continuity branches.
* Proposed patches.
* Captain merge.
* Per-topic revision conflicts.
* Worker-completion fold.
* Evidence links into AgentTeams reports and task records.

Goal: separate shared conceptual state from raw worker transcripts.

## Phase 6: Selection Policy and Evaluation

Improve preparation-packet selection using:

* Explicit topic priority.
* Dependency reachability.
* Recent access.
* Open blockers.
* Model-requested focus.
* Token budgets.
* Retrieval history.

Goal: reduce packet size while preserving successful continuation.

---

# 15. MVP Acceptance Tests

## Continuity

* A successor can explain the current task model without receiving the predecessor transcript.
* A successor can name the next executable action.
* A successor distinguishes verified, assumed, inherited, and unknown claims.
* Superseded hypotheses do not remain prominent.
* Full historical evidence remains retrievable.

## Cache and Context

* Continuity updates do not rebuild the active epoch context.
* Prefix-cache behavior inside an epoch remains unchanged.
* Preparation packets serialize deterministically.
* A new epoch receives substantially fewer tokens than transcript replay.
* Volatile material appears after stable prefix material.

## Handoff

* Killing an epoch after an arbitrary tool step leaves a mostly usable graph.
* Graceful stop produces a seal without reconstructing the entire session.
* The seal marks inherited entries that were not rechecked.
* Repository and task state match the seal.

## Async Messaging

* A user can message an active agent without ending its epoch.
* The agent can reply without stopping work.
* Waiting for a user consumes no model calls.
* Urgent messages are delivered at the next safe seam.
* Normal messages do not interrupt atomic operations.

## Concurrency

* Stale topic patches are rejected.
* Worker proposals cannot directly overwrite captain-owned topics.
* Conflicting worker proposals remain separately inspectable.
* Captain merge records provenance.

## Rescue

* A crash with an unsealed tail starts a rescue epoch.
* The rescue epoch identifies incomplete tool operations.
* Recovered facts are marked as recovered, not silently verified.
* Ordinary implementation does not resume until a rescue seal exists.

---

# 16. Evaluation Metrics

Operational metrics matter more than benchmark mythology.

Track:

* Useful work per input token.
* Useful work per dollar.
* Cache hit ratio within epochs.
* Cold-start cost per epoch.
* Average epoch duration.
* Preparation-packet size.
* Continuity graph size.
* Ratio of active to dormant topics.
* Number of rediscovered facts.
* Number of stale inherited claims relied upon.
* Premature completion rate.
* Rescue success rate.
* Soft-stop completion rate.
* User-message response latency.
* Time spent blocked without useful work.
* Worker-report compression ratio.
* Semantic merge conflicts.
* Reviewer complaints attributable to lost context.
* Successor time to first useful action.

A promising system should improve both average output and failure-tail behavior.

---

# 17. Major Design Questions

1. What should trigger an epoch boundary besides compaction?
2. Should major phase transitions automatically create successors?
3. How large should a preparation packet be?
4. Should the model explicitly select packet topics before sealing?
5. Which continuity operations belong in a tool versus hidden sideband?
6. How should sideband syntax survive malformed model output?
7. Should graph storage begin as files or SQLite?
8. How should user-authored constraints be protected from model supersession?
9. Can a worker archive a topic, or only propose archival?
10. How should topic staleness be evaluated automatically?
11. Should repository commits and test results become first-class evidence nodes?
12. How should DSH compaction coexist with profiles that do not opt into succession?
13. How can Anchored Standard preserve its first-turn trajectory while enabling continuity later?
14. What minimum seal is acceptable under severe provider preemption?
15. How much of an unsealed continuity patch should rescue trust?
16. Should a successor inherit the predecessor's provider and model by default?
17. When should the harness deliberately route a successor to a different model?
18. How should private worker branches be garbage-collected after merge?
19. Should the user be able to inspect and edit the continuity graph directly?
20. How should secrets and sensitive tool output be redacted from preparation packets?

---

# 18. Recommended First Coding Task

Build the smallest vertical slice that demonstrates succession without transcript replay.

## Scope

1. Add a Cordis plugin with a file-backed continuity store.
2. Create one job automatically for an opted-in DSH session.
3. Expose:

   * `continuity_read`
   * `continuity_patch`
   * `continuity_seal`
4. Store topic nodes with revisions and epistemic state.
5. Add a command or control action:

   * `continuity_start_successor`
6. On successor start:

   * Do not replay the old transcript.
   * Compile a deterministic preparation packet.
   * Start a fresh model context under the same workspace.
7. Preserve the old transcript as journal history.
8. Run one real repository task across a forced succession boundary.

## Demonstration Scenario

1. Give the agent a substantial refactor.
2. Allow reconnaissance and partial implementation.
3. Force a succession boundary before completion.
4. Start a fresh successor from the preparation packet.
5. Have the successor continue implementation and verification.
6. Run an adversarial review.
7. Compare:

   * Resume quality.
   * Input tokens.
   * Time to first useful successor action.
   * Lost assumptions.
   * Repeated investigation.
   * Final review findings.

## Success Condition

The successor resumes the real task competently without receiving the predecessor transcript, while retaining access to exact historical evidence through topic and journal retrieval.

That result would validate the central abstraction before adding async messaging, hidden sidebands, rescue, or multi-agent graph merging.

---

# Summary

The durable identity of agentic work should belong to the **job**, not to a chat transcript or one model context.

Each epoch is a temporary worker:

1. Receive a preparation packet.
2. Work through a cache-friendly append-only stream.
3. Maintain a structured write-ahead handoff.
4. Communicate through asynchronous mailboxes.
5. Mark evidence and uncertainty.
6. Drain and certify when ending.
7. Hand the job to a successor.

The continuity graph provides a mutable foreground. The journal preserves the past. The preparation packet selects the present. The seal makes succession explicit.

This is less a memory feature than a process model for long-running agents.