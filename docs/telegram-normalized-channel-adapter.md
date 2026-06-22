# Telegram Normalized Channel Adapter

Telegram is an optional channel provider in Rusty Crew. It maps Telegram Bot API chats, topics, users, messages, and outbound sends into the shared normalized channel contract. It does not introduce Telegram-specific Rust messaging behavior.

## Scope

The TypeScript adapter owns:

- Telegram Bot API client abstraction;
- update normalization;
- chat/topic/user identity mapping;
- outbound `sendMessage` request formatting;
- Telegram binding creation from chat metadata.

Rust continues to own:

- normalized external events;
- routed messages;
- wake policy;
- durable runtime state.

## Normalized Mapping

`telegramBindingFromChat` creates a `ChannelBindingRecord` with `provider: "telegram"`.

`normalizeTelegramUpdate` maps supported update shapes into `NormalizedChannelInboundMessage`:

- `message`
- `edited_message`
- `channel_post`
- `edited_channel_post`

Telegram chat id becomes `externalChannelId`. Telegram forum topic id becomes `externalThreadId`. Telegram message id becomes `externalMessageId`. The update id is used as the cursor.

`toTelegramSendMessageRequest` formats a `NormalizedChannelOutboundMessage` into Bot API `sendMessage` arguments, preserving chat id, thread id, reply target, text body, and safe default link-preview behavior.

## Verification

Run:

```bash
npm run smoke:telegram
npm run smoke:telegram-service-connector
npm run smoke:telegram-live
```

The smoke proves binding creation, update normalization, mentions, attachment refs, outbound formatting, and adapter wrapper send behavior.

`smoke:telegram-service-connector` proves the hosted connector path with a mocked Bot API: long-poll `getUpdates`, durable offset advancement, normalized ingress routing, unbound update quarantine, outbound projection, and `sendMessage` formatting.

`smoke:telegram-live` is skipped by default. Set `RUSTY_CREW_TELEGRAM_LIVE_SMOKE=true` and `RUSTY_CREW_TELEGRAM_BOT_TOKEN` to run a safe live `getUpdates` check.

## Hosted Connector

Rusty Crew can run a hosted Telegram connector around this adapter. It is disabled by default and enabled through service env/config:

- `RUSTY_CREW_TELEGRAM_ENABLED=true`
- `RUSTY_CREW_TELEGRAM_BOT_TOKEN=<operator-provided token>`
- `RUSTY_CREW_TELEGRAM_API_BASE_URL` (optional; defaults to Telegram Bot API)
- `RUSTY_CREW_TELEGRAM_ADAPTER_ID` (optional; defaults to `telegram-main`)
- `RUSTY_CREW_TELEGRAM_POLL_INTERVAL_MS`
- `RUSTY_CREW_TELEGRAM_POLL_TIMEOUT_SECONDS`
- `RUSTY_CREW_TELEGRAM_UPDATE_LIMIT`
- `RUSTY_CREW_TELEGRAM_MESSAGE_TTL_MS`

The first implementation uses long polling rather than webhooks. Processed Telegram update offsets are persisted to service data storage so restarts do not replay old Telegram messages. Unbound or ambiguous chats/topics are counted in diagnostics and advanced past; they are not routed to a fallback agent.

Bindings are still explicit `ChannelBindingRecord` entries. A Telegram binding should use `provider: "telegram"`, the configured Telegram adapter id, `externalChannelId` as the Telegram chat id, and `externalThreadId` as the forum topic/message thread id when needed.
