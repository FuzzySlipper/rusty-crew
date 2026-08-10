# Telegram install diplomat operator runbook

This runbook provisions one Telegram bot for one Rusty Crew installation and
binds it to one exact full Crew session. Repeat the procedure independently on
each installation that participates in the shared support group.

The bot is an external conversation adapter. The binding is session-scoped and
movable; it is not profile configuration, does not constrain a profile or
working directory, and does not add Telegram-specific permissions. The bound
session's normal harness and tool permissions remain authoritative.

## Before you start

For each installation, have:

- a distinct Telegram bot and token;
- a running Rusty Crew service with `RUSTY_CREW_TELEGRAM_ENABLED=true`;
- Rusty View connected to that exact service;
- one existing full Crew session chosen as the diplomat;
- the private supergroup and, if used, forum topic where the bot will work.

Use a distinct bot per installation. Do not copy one bot token between Crew
services. Telegram file handles are also bot-scoped and must not be copied
between installations.

## 1. Create and configure the bot

1. In `@BotFather`, run `/newbot`, choose a recognizable installation name and
   a unique username, and retain the issued token outside chat transcripts and
   repository files.
2. Open the bot's settings and enable **Bot-to-Bot Communication Mode**.
3. Leave **Group Privacy Mode** enabled for the normal
   `mention_or_reply` setup. With privacy enabled, humans should mention the
   bot or reply to one of its messages. Disabling privacy is only needed for a
   deliberate `topic_human_messages` binding and requires removing and
   re-adding the bot before Telegram applies the change.
4. Allow the bot to join groups, add it to the private supergroup, and add it
   to the intended forum topic if topics are used. Group administrator status
   is not required for `mention_or_reply`.

Telegram's current Bot API supports bot-authored group messages addressed by a
command mention or direct reply when Bot-to-Bot Communication Mode is enabled.
Rusty Crew still applies its durable correlation, duplicate, depth, lifetime,
and bot-pair budgets before waking a diplomat.

Official references:

- [Telegram bot creation and privacy settings](https://core.telegram.org/bots/features#botfather)
- [Telegram bot-to-bot communication](https://core.telegram.org/api/bots/bot-to-bot)
- [Telegram Bot API](https://core.telegram.org/bots/api)

## 2. Enable the adapter and store the token

Set the service bootstrap configuration and restart the installation normally:

```text
RUSTY_CREW_TELEGRAM_ENABLED=true
RUSTY_CREW_TELEGRAM_CREDENTIAL_ID=telegram-main
```

Do not place the token in a checked-in environment file. In Rusty View, open
**Service > Telegram**, enter the token in **Rotate token**, and apply it. The
service stores the raw token through its credential store; readback exposes
only redacted credential metadata and the `getMe` bot identity.

The connector uses outbound long polling. It does not require an inbound
webhook, tunnel, or public service endpoint. Remove any webhook previously
configured for the bot because Telegram does not allow `getUpdates` while a
webhook is active.

Expected readback progression is `disabled` before service enablement,
`unconfigured` before a token is stored, and then `unbound` after bot identity
is verified but no route exists. `disconnected` or `rate_limited` requires
diagnosis before binding.

## 3. Discover and bind the Telegram surface

1. Send one addressed message to the bot in the intended group/topic so the
   connector observes that surface.
2. Refresh **Service > Telegram** and select the observed group and optional
   topic. Verify the bot id/username and sanitized chat/topic identity.
3. Select the exact full Crew session. Confirm that installation, bot, group,
   topic, session, agent, profile, and workdir appear as separate readout
   fields.
4. Choose a participation mode:
   - `mention_or_reply` is the default for private groups and privacy-enabled
     bots;
   - `topic_human_messages` accepts unaddressed human messages only in the
     exact bound topic. It does not make bot messages or Telegram identities
     privileged.
5. Create the binding and retain its binding id and revision in the evidence
   packet. A healthy installation has one unambiguous exact
   `bot + chat + optional topic -> session` route.

The API equivalent is documented in
`telegram-diplomat-admin-api-v0.openapi.json`; use the generated contract
rather than inventing request shapes.

## 4. Prove the conversation path

Run these checks in order and inspect both Telegram and Rusty View:

1. Human A mentions bot A; only diplomat A receives and answers it.
2. Human B replies to bot B; only diplomat B receives and answers it.
3. A human starts a correlated exchange, then diplomat A addresses or replies
   to bot B. Verify the exchange terminates inside the configured bot-loop
   budget.
4. Ask one diplomat to consult a local Crew specialist that has no Telegram
   identity. Verify the specialist reply returns through Crew coordination and
   the diplomat authors the Telegram response.
5. Ask a diplomat to run one ordinary, authorized local diagnostic or
   reversible adjustment. Telegram grants no additional authority.
6. Send a screenshot or document with a useful question. Verify that the
   resulting Crew artifact has sender/chat/topic/message provenance and that
   the diplomat actually inspects it.

An unaddressed message in `mention_or_reply`, a message for the other bot, an
edit, and an unsupported update must be visibly ignored or quarantined rather
than executed by a fallback session.

## 5. Restart, outage, and replay checks

Before each disruption, capture the binding revision, durable cursor, latest
provider message id, outbox state, and connector counters.

### Service restart

Restart one Crew service through its normal service manager. After restart:

- the same binding and exact session are present;
- the cursor resumes beyond the last terminal update;
- already handled Telegram messages are not replayed;
- local Crew coordination remains available while Telegram reconnects.

### Short and extended outage

1. Stop one installation's Telegram connector or Crew service while leaving
   the other installation running.
2. Generate addressed traffic during the outage.
3. Restore it once inside the configured message TTL/replay window and verify
   ordered recovery without duplicate execution.
4. Repeat beyond the window and verify explicit expiry/quarantine diagnostics
   rather than a fabricated reply.

Telegram transport failure must degrade only that external adapter. It must
not interrupt local Crew messaging, tools, delegation, or the other install.

## 6. Failure and loop exercises

Record diagnostics before and after each exercise:

- replay the same update/message identity and verify one execution plus a
  duplicate terminal receipt;
- create an intentionally ambiguous candidate surface, verify `ambiguous`, and
  remove the ambiguity before resuming;
- drive a bounded bot reply loop and verify a terminal depth, message-count,
  lifetime, or bot-pair-rate reason;
- induce a retryable outbound failure, including provider retry guidance where
  practical, and verify pending/retry then one terminal delivery receipt;
- archive or replace the bound session and verify `needs_rebind` with no
  profile fallback;
- deliberately move the binding with its current `expectedRevision`, verify
  the new exact session receives subsequent messages, and verify neither
  session, profile, nor workdir was mutated or archived.

## 7. Token rotation, pause, and removal

- **Rotate:** use **Service > Telegram > Rotate token**. Confirm the new bot
  identity before resuming. A failed identity/reload moves active bindings to
  `needs_rebind`; it does not silently keep a misleading healthy route.
- **Pause/resume:** use the revisioned binding controls when conversation
  routing should stop without deleting state.
- **Move:** select a new exact session and apply the current revision. This
  changes only the binding target.
- **Remove:** remove the binding, then remove or rotate the stored credential.
  Removing a binding never archives the diplomat session.
- **Telegram cleanup:** remove the bots from the test group and revoke test
  tokens in `@BotFather` when the topology is no longer needed.

## Diagnostics checklist

Capture the Rusty View Telegram panel or
`GET /v1/admin/telegram-diplomat` readback with secrets and direct personal
identifiers sanitized. Include:

- service state, adapter id, credential revision, and verified bot identity;
- installation label plus exact binding/session/agent/profile/workdir fields;
- chat/topic and participation mode;
- cursor, poll, retry, duplicate, ignored, quarantine, and last-error state;
- loop depth/rate/expiry outcomes;
- outbox attempts, provider message ids, and terminal delivery state;
- media detection, download, checksum/artifact, rejection, and size counters.

## Certification evidence template

```text
Rusty Crew install A SHA / ref:
Rusty Crew install B SHA / ref:
Rusty View SHA / ref:
GitHub required checks and terminal results:
Bot API version observed:
BotFather settings (A/B):
Sanitized group/topic topology:
Sanitized bot/human/session/profile/workdir identities:
Provisioning and binding commands/actions:
Human mention/reply evidence:
Bot-to-bot correlated exchange evidence:
Local specialist round evidence:
Authorized diagnostic/adjustment evidence:
Media inspection evidence:
Restart and no-replay evidence:
Inside-window outage recovery evidence:
Beyond-window degradation evidence:
Duplicate/loop/retry/ambiguous/rebind evidence:
Local coordination during dual adapter failure:
Telegram and Rusty View screenshots inspected:
Transcript and diagnostic artifact paths:
Cleanup performed:
Residual limitations:
```

Do not call the campaign certified until the two-install evidence is generated
and its Telegram and Rusty View screenshots/transcripts are actually inspected.
