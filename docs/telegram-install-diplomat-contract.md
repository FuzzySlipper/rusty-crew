# Telegram Install Diplomat Contract

Status: accepted implementation contract for campaign 6763
Version: `telegram_install_diplomat.v1`
Date: 2026-08-10

This document defines how one Rusty Crew installation participates in a
shared Telegram support room through one install diplomat. It supersedes
provider-specific assumptions in `telegram-normalized-channel-adapter.md` when
they conflict. The normalized channel contract and Rusty Crew unified
architecture still govern the provider-neutral boundary.

## Outcome

Each installation has one Telegram bot identity and one designated full Crew
session acting as its diplomat. Humans and diplomats from other installations
address that bot in a private Telegram supergroup or forum topic. The diplomat
may consult local Crew agents through the normal Rust coordination bus and then
reply as the installation's single Telegram identity.

Telegram is a conversation adapter. It is not a remote shell, an internal
agent bus, a profile owner, or an alternate operator authority. A diplomat can
inspect or adjust its machine only through the same tools, MCP bindings, and
harness permissions that its session has when used from Rusty View.

The bound Telegram chat/topic is an operator-selected conversation input, not
an identity-authentication boundary. Campaign 6763 deliberately does not add a
per-sender Telegram allowlist, map Telegram users to Crew operators, or make
Telegram identity an approval credential. A human message delivered from the
exact bound surface is treated like user input submitted to that diplomat
session; mention/reply policy controls participation, not authorization.
Actions still pass through the session's existing harness/tool approval and
permission behavior. Deployments that require authenticated or multi-user
remote operation need a separate, service-wide operator identity design; it
must not be introduced here as Telegram-specific precautionary policy.

The supported default transport is Bot API long polling. A remote installation
therefore needs outbound internet access but no inbound webhook, Cloudflare
tunnel, or reachable Rusty View instance.

## Product Topology

```text
private Telegram supergroup / forum topic
  human operator(s)
  installation A bot -> installation A diplomat session -> local Crew agents
  installation B bot -> installation B diplomat session -> local Crew agents
```

There is one bot token per installation. A token has one authoritative update
consumer. Installations must not share a token or compete for its Bot API
update cursor.

A single installation may bind its diplomat to multiple deliberate Telegram
surfaces later, but campaign 6763 certifies one active group/topic binding for
one bot and one diplomat session. The adapter framework remains multi-binding;
the product must not introduce a process-global "current agent."

## Authority And Ownership

### Rust owns

- the durable diplomat/channel binding and its revision;
- the target agent, instance, and session identity;
- route selection, duplicate/expiry decisions, wake eligibility, and bot-loop
  budget decisions;
- correlation between inbound Telegram messages, local Crew collaboration,
  and outbound Telegram replies;
- binding re-targeting, restart hydration, archived-session degradation, and
  delivery/outbox lifecycle;
- Crew transcript, attachment metadata, diagnostics, and runtime events.

### The TypeScript Telegram adapter owns

- Bot API request/response shapes and transport errors;
- long polling and provider update normalization;
- Telegram chat, topic, message, user, bot, reply, entity, and file mapping;
- provider formatting, message splitting, file download/upload, and rate-limit
  observations;
- projection of Rust-owned delivery intentions and receipts.

### Rusty View owns

- installation-level bot setup and token rotation controls;
- bot identity verification and observed group/topic selection;
- explicit diplomat-session binding, move, pause, resume, and removal UX;
- healthy/degraded diagnostics and recovery affordances;
- visible separation of installation, bot, session, workdir, and profile.

### Profiles do not own

- the Telegram bot token;
- the group/topic binding;
- the diplomat designation;
- a working-directory restriction or Telegram-specific authority policy.

`profile_id` remains useful readback for the selected session and its tool
composition. It is not the lifecycle key for the diplomat binding.

## Durable Binding

The Rust-owned binding is versioned and revision-safe. Its logical shape is:

```text
telegram_install_diplomat_binding.v1
  binding_id
  revision
  installation_id
  installation_label
  adapter_id
  bot_user_id
  bot_username
  agent_id
  instance_id
  session_id
  external_chat_id
  external_thread_id?
  participation_mode
  status
  degraded_reason?
  created_at
  updated_at
```

The binding never stores the bot token. The token stays in the established
service secret/configuration surface. Provider IDs remain strings even when
Telegram currently represents them as integers.

`participation_mode` starts with:

- `mention_or_reply`: the default, compatible with Telegram group privacy
  mode and bounded support conversations;
- `topic_human_messages`: human messages in the exact bound topic may wake the
  diplomat, while bot messages still require an addressed reply/mention or an
  active correlated exchange.

Neither mode classifies Telegram humans as authorized or unauthorized. The
operator establishes the intended audience when binding the exact private
chat/topic. Rust wake policy may reject unaddressed, unbound, duplicate,
expired, or loop-budgeted input, but it does not maintain a Telegram-human
permission list.

There is no implicit all-groups or all-topics fallback. Unbound updates are
visible terminal classifications, not guessed routes.

Moving a binding changes only its agent/instance/session target and revision.
It does not mutate either session's profile or workdir and does not archive the
old session. If the target session is archived or unavailable, the binding
becomes visibly `needs_rebind`/degraded and does not silently select another
session.

## Telegram Identity And Inbound Envelope

The provider-neutral inbound shape must preserve enough Telegram identity to
distinguish a human, a bot, and a chat acting as sender. The install-diplomat
extension carries:

```text
telegram_sender.v1
  kind: human | bot | sender_chat
  user_id
  username?
  display_label?
  is_bot

telegram_message_context.v1
  update_id
  chat_id
  thread_id?
  message_id
  edit_date?
  reply_to_message_id?
  sender
  entities[]
  attachment_refs[]
```

Mention routing uses Telegram message entities when present and a text parser
only as a compatibility fallback. Usernames are normalized case-insensitively
for lookup while original spelling remains available for display.

The Crew-authored user message exposes the Telegram sender's display identity
and stable external ID. It must not collapse every Telegram participant to a
generic debug/system user. Bot-originated messages remain visibly bot-authored
inside the diplomat wake and transcript.

An edited message is not a new ordinary turn with the same idempotency key. It
is classified as an edit referencing the original external message. Campaign
6763 may initially expose edits as non-executable context/diagnostics, but it
must not silently drop them or execute the old and edited text as unrelated
requests. Unsupported update kinds are explicitly ignored or quarantined with
a reason code.

## Addressing And Participation

The default group interaction is deliberate:

- a human mentions the installation bot or replies to one of its messages;
- one diplomat replies to another bot's message or uses an addressed command;
- a human message begins a fresh interaction budget;
- a bot message can continue only a correlated, non-terminal interaction;
- ordinary local Crew agents are reached by the diplomat over Crew messaging
  or delegation and do not need Telegram identities.

Telegram Bot-to-Bot Communication Mode must be enabled for participating bots.
Keeping group privacy mode enabled is the recommended initial setup. Disabling
privacy mode or granting group administration may broaden provider delivery,
but it must not broaden Rust wake policy.

Outbound replies preserve `reply_to_message_id` whenever a concrete inbound
message caused the response. A reply from a local specialist is not projected
directly. It returns to the diplomat's correlated Crew round, and the diplomat
owns the Telegram response.

## Bot-Loop Termination

Telegram requires bots using Bot-to-Bot Communication Mode to terminate
predictably. TTL and duplicate checks are necessary but not sufficient.

Rust keeps a durable interaction record keyed by binding and interaction ID.
It records the external parent message, sender/receiver bot pair, current
depth, message count, deadline, and terminal reason. Provider reply IDs and
Crew correlation IDs connect subsequent messages to that record.

Initial defaults for certification are:

- maximum bot-to-bot depth: 6;
- maximum bot-authored messages in one interaction: 8;
- maximum interaction lifetime: 5 minutes;
- maximum bot-pair messages per group/topic: 8 per minute;
- no autonomous restart after a terminal reason without a new human message.

The limits are service-level operational settings, not profile restrictions or
project allowlists. Rejection emits a terminal diagnostic such as
`telegram_bot_loop_depth_exceeded`, `telegram_bot_pair_rate_limited`, or
`telegram_bot_interaction_expired`. It does not interrupt local Crew routing.

## Cursor, Retry, And Delivery Contract

`update_id` is the Telegram transport cursor. The external message ID plus
chat/topic is the message idempotency identity.

The connector may advance the durable update cursor after:

- a durable routed/duplicate/expired/denied receipt;
- a durable unbound or unsupported quarantine receipt;
- a transient failure has exhausted a bounded retry policy and a durable
  quarantine receipt exists.

It must not advance in an unconditional `finally` after an unrecorded transient
ingress failure. A poison update must not block the queue forever: bounded
retry ends in visible quarantine, then the cursor advances.

Outbound messages use a Rust-owned outbox intent with an idempotency key.
Telegram success produces a delivery receipt containing the external message
ID. Retryable failures, including `429` with provider retry guidance, remain
pending until policy terminates them. Permanent failure becomes a visible
terminal receipt. Draining a Rust event before a failed `sendMessage` is not a
delivery guarantee.

Long text is split into bounded Telegram messages without breaking the logical
interaction/correlation. Streaming is not required for the first campaign
delivery; coherent final and checkpoint messages are preferred over projecting
token-level output.

## Attachments And Artifacts

Telegram `file_id` is a provider handle scoped to one bot, not a Crew artifact
and not portable across installation bots.

For supported inbound media the adapter obtains file metadata and bytes through
the Bot API, while Rust records the attachment in Crew-owned artifact storage
with sender/chat/topic/message provenance, MIME type, byte size, checksum, and
retention metadata. The diplomat receives the normal Crew attachment handle,
not a giant inline body or an unresolved `telegram:file:` pseudo-URL.

Unsupported, expired, interrupted, or oversized media is visible as a degraded
attachment block. Outbound files are uploaded or linked per delivery policy;
an inbound Telegram file ID from installation A is never reused by installation
B.

## Current-State Audit

| Surface | Classification | Contract consequence |
| --- | --- | --- |
| `adapter-telegram` Bot API HTTP client | Reusable but narrow | Keep `getUpdates`/`sendMessage`; add bot identity, entities, files, retry metadata, and current update shapes. |
| `TelegramChannelConnector` long polling | Reusable skeleton | Keep outbound-only network topology; replace unconditional cursor advancement after transient failures with durable terminal receipts/quarantine. |
| File update-offset store | Transitional | Useful for current smoke/single process, but diplomat cursor/outbox authority must move behind Rust-owned persistence. |
| Telegram normalization and `sendMessage` projection | Reusable baseline | Preserve chat/topic/reply mapping; add bot sender kind, usernames, edit semantics, entity-aware mentions, delivery result IDs, and bounded splitting. |
| Attachment `telegram:file:<id>` refs | Incomplete | They prove detection only; task 6767 must resolve bytes into Crew attachment/artifact storage. |
| Rust `plan_channel_ingress_route` | Reusable authority | Extend its typed input/decision surface for sender kind, reply/correlation, participation mode, durable duplicate evidence, and loop budget. |
| TS route resolver | Compatibility/test duplicate | Production must continue through the Rust planner; remove or confine duplicate policy after conformance coverage no longer needs it. |
| `ChannelBindingRecord` persistence repositories | Implemented but unwired to hosted Telegram lifecycle | Use these Rust stores as binding authority; stop treating authored TS runtime-config arrays as the durable diplomat record. |
| Runtime-config `channelBindings` and session-replacement rewrite | Transitional | Preserve explicit move semantics, but supported APIs must perform revision-safe Rust-owned binding mutations instead of raw config-file ownership. |
| Service Telegram startup/outbound drain | Reusable composition skeleton | Keep adapter registration and degraded isolation; add supported admin lifecycle and Rust-owned outbox/delivery receipts. |
| Environment-only token configuration | Reusable bootstrap, incomplete product UX | Keep secret separation; add supported setup, identity verification, rotation, and reload APIs/UI. |
| Connector diagnostics | Reusable baseline | Extend with bot identity, binding revision, cursor/quarantine, loop, rate, outbox, media, and last delivery state. |
| Existing deterministic smokes | Reusable regression baseline | They cover single-bot text routing/restart only; retain them and add multi-bot/human/failure conformance. |
| Existing live smoke | Incomplete certification | `getUpdates` reachability is not install-diplomat proof; task 6770 owns two-install rendered certification. |
| Rusty View | Missing | Task 6769 owns installation setup, session binding, switching, diagnostics, and recovery UI. |

## Degraded Behavior

- Telegram transport failure marks the adapter disconnected/degraded; local
  Crew messages, tools, and delegation continue.
- Missing token is `unconfigured`, distinct from disabled or disconnected.
- An unknown chat/topic is `unbound`; multiple viable targets are `ambiguous`.
- A missing/archived session is `needs_rebind`; no profile fallback occurs.
- A stale/duplicate update is terminal and non-executable.
- A retryable ingress or delivery failure remains pending or becomes durable
  quarantine after bounded attempts; it is not reported as delivered.
- Loop/rate termination is visible in both service diagnostics and the bound
  conversation when a safe reply can be sent.
- Attachment failure does not discard the surrounding text message; the
  attachment is represented as degraded evidence.

No adapter failure rolls back or blocks Rust-owned internal coordination.

## Administration Contract

Supported service operations must cover:

1. configure/rotate/remove a secret reference and reload the connector;
2. verify the bot identity (`getMe`) without returning its token;
3. list observed candidate groups/topics with bounded provider metadata;
4. create/read/move/pause/resume/archive a revisioned diplomat binding;
5. inspect connector, cursor, quarantine, loop, outbox, media, and last-error
   diagnostics;
6. deliberately recover or rebind after session archival/replacement.

Rusty View consumes generated contracts for these operations. It must show bot,
installation, group/topic, session, workdir, and profile as separate fields.

## Implementation Ownership By Campaign Task

- **6764:** this contract and current-state classification.
- **6765:** Telegram Bot API normalization, bot-to-bot addressing, loop/retry
  transport behavior, message formatting, and conformance.
- **6766:** Rust-owned diplomat/session identity, durable interaction policy,
  local Crew consultation, and session lifecycle.
- **6767:** Telegram file resolution and Crew artifact integration.
- **6768:** supported provisioning, revisioned binding, diagnostics, and token
  lifecycle APIs plus generated contracts.
- **rusty-view/6769:** installation setup and operations UI.
- **6770:** two-install live certification and operator runbook.

## Required Acceptance Scenarios

Deterministic and live evidence together must show:

1. two installations, bot identities, diplomat sessions, and humans in one
   private group;
2. mention/reply routing to the intended installation;
3. correlated bot-to-bot exchange that terminates within its budget;
4. a diplomat consulting a non-Telegram local agent and replying as itself;
5. ordinary local diagnostics/tools subject to the diplomat session's existing
   harness permissions and approvals;
6. exact session rebind without profile/workdir mutation;
7. restart cursor/binding/outbox hydration without replay;
8. offline recovery within and beyond the replay/TTL window;
9. duplicate, ambiguous, rate-limited, delivery-failed, and quarantined
   diagnostics;
10. one real screenshot or document made model-inspectable through Crew
    artifacts;
11. Telegram/Rusty View rendered evidence and cleanup at exact reviewed SHAs.

## Explicit Non-Goals

- Exposing every local Crew agent as a Telegram bot.
- Making Telegram a replacement for Rust's internal bus or canonical history.
- Making a bot token or channel binding profile-owned.
- Adding profile working-directory confinement, project allowlists, or a
  Telegram-only privilege model.
- Adding a Telegram-user allowlist or treating Telegram sender identity as a
  Crew operator credential; service-wide multi-user authentication is separate
  future work.
- Requiring inbound webhooks, public local services, or network tunnels.
- Treating Telegram file handles or chat history as Crew-owned durable state.
