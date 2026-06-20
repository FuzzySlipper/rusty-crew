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
```

The smoke proves binding creation, update normalization, mentions, attachment refs, outbound formatting, and adapter wrapper send behavior.
