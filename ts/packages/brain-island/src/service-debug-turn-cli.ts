import { createDebugApiClient } from "./debug-api-client.js";

const [sessionId, ...bodyParts] = process.argv.slice(2);
const body = bodyParts.join(" ").trim();

if (!sessionId || !body) {
  console.error(
    "Usage: npm run service:debug-turn -- <session-id> <message...>",
  );
  process.exit(2);
}

const baseUrl =
  process.env.RUSTY_CREW_ADMIN_BASE_URL ?? "http://127.0.0.1:9347";
const bearerToken = process.env.RUSTY_CREW_ADMIN_TOKEN;
const client = createDebugApiClient({
  baseUrl,
  bearerToken,
  timeoutMs: Number.parseInt(
    process.env.RUSTY_CREW_DEBUG_TURN_TIMEOUT_MS ?? "120000",
    10,
  ),
  retries: 0,
});

const outcome = await client.requestDirectDebugTurn({
  sessionId,
  actorId: process.env.RUSTY_CREW_DEBUG_ACTOR_ID ?? "local-operator",
  body,
  reason: "service debug turn CLI",
});

console.log(
  JSON.stringify(
    {
      baseUrl,
      sessionId,
      status: outcome.status,
      wakeId: outcome.wakeId,
      messageId: outcome.messageId,
      reasonCode: outcome.reasonCode,
      summary: outcome.summary,
    },
    null,
    2,
  ),
);
