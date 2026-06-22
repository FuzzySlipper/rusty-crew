import assert from "node:assert/strict";
import { createTelegramBotApiHttpClient } from "./index.js";

const enabled = process.env.RUSTY_CREW_TELEGRAM_LIVE_SMOKE === "true";
if (!enabled) {
  console.log(
    JSON.stringify(
      {
        skipped: true,
        reason: "set RUSTY_CREW_TELEGRAM_LIVE_SMOKE=true to run",
      },
      null,
      2,
    ),
  );
  process.exit(0);
}

const token = process.env.RUSTY_CREW_TELEGRAM_BOT_TOKEN?.trim();
assert.ok(
  token,
  "RUSTY_CREW_TELEGRAM_BOT_TOKEN is required for live Telegram smoke",
);

const client = createTelegramBotApiHttpClient({
  token,
  baseUrl: process.env.RUSTY_CREW_TELEGRAM_API_BASE_URL,
  timeoutMs: 10_000,
});
const updates = await client.getUpdates?.({ limit: 1, timeout: 0 });
assert.ok(Array.isArray(updates), "getUpdates should return an array");

console.log(
  JSON.stringify(
    {
      skipped: false,
      updateCount: updates.length,
    },
    null,
    2,
  ),
);
