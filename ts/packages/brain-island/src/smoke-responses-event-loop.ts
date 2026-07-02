import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "./service-host.js";

const tmpRoot = join(tmpdir(), "rusty-crew");
mkdirSync(tmpRoot, { recursive: true });

const root = mkdtempSync(join(tmpRoot, "responses-event-loop-"));
const port = await openPort();
const fakeDelayMs = Number(
  process.env.RUSTY_CREW_OPENAI_RESPONSES_EVENT_LOOP_FAKE_DELAY_MS ?? "800",
);
const originalFakeDelay = process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS;
process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS = String(fakeDelayMs);

writeRuntimeConfig(root);
const host = await startRustyCrewServiceHost({
  env: {
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
    RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
    RUSTY_CREW_ADMIN_PORT: String(port),
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "60000",
    RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "50",
  },
});

try {
  const client = createDebugApiClient({
    baseUrl: `http://127.0.0.1:${port}`,
    retries: 0,
    timeoutMs: 2_000,
  });
  let turnSettled = false;
  const turnStartedAt = performance.now();
  const turnPromise = client
    .requestDirectDebugTurn({
      sessionId: "responses-event-loop-session",
      actorId: "local-operator",
      body: "Run a deliberately slow fake Responses wake.",
      reason: "responses event-loop responsiveness smoke",
    })
    .finally(() => {
      turnSettled = true;
    });

  await sleep(25);
  const preAdminDelayMs = performance.now() - turnStartedAt;
  assert.ok(
    preAdminDelayMs < Math.max(250, fakeDelayMs / 2),
    `event loop was blocked before admin request: waited ${Math.round(
      preAdminDelayMs,
    )}ms before diagnostics during a ${fakeDelayMs}ms fake wake`,
  );
  assert.equal(
    turnSettled,
    false,
    "responses turn finished before diagnostics could run; smoke did not observe an in-flight wake",
  );

  const adminStartedAt = performance.now();
  const diagnostics = await client.diagnostics();
  const adminDurationMs = performance.now() - adminStartedAt;
  assert.equal(typeof diagnostics.overview.generatedAt, "string");
  assert.equal(
    turnSettled,
    false,
    "responses turn completed before admin diagnostics returned; smoke did not prove concurrent responsiveness",
  );
  assert.ok(
    adminDurationMs < Math.max(250, fakeDelayMs / 2),
    `admin diagnostics was delayed ${Math.round(
      adminDurationMs,
    )}ms during in-flight fake responses wake`,
  );

  const turn = await turnPromise;
  const turnDurationMs = performance.now() - turnStartedAt;
  assert.equal(turn.status, "accepted");
  assert.match(turn.summary, /responses replay (service )?wake completed/);
  assert.ok(
    turnDurationMs >= fakeDelayMs,
    `fake responses wake completed too quickly: ${Math.round(
      turnDurationMs,
    )}ms < ${fakeDelayMs}ms`,
  );

  const chatStreamAbort = new AbortController();
  const chatStreamResponse = await fetch(
    `http://127.0.0.1:${port}/v1/chat/sessions/responses-chat-stream-session/stream`,
    { signal: chatStreamAbort.signal },
  );
  assert.equal(chatStreamResponse.status, 200);
  assert.equal(
    chatStreamResponse.headers
      .get("content-type")
      ?.includes("text/event-stream"),
    true,
  );
  const streamedEventsPromise = collectSseEventsUntil(
    chatStreamResponse,
    (events) => events.some((event) => event.kind === "assistant_text_delta"),
    chatStreamAbort,
  );
  let chatPostSettled = false;
  const chatStartedAt = performance.now();
  const chatPostPromise = fetch(
    `http://127.0.0.1:${port}/v1/chat/sessions/responses-chat-stream-session/messages`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": "responses-chat-stream-1",
      },
      body: JSON.stringify({
        actor: { id: "human-operator", kind: "human" },
        body: "prove Rust brain stream reaches chat SSE before POST settles",
        client_message_id: "responses-chat-stream-message-1",
      }),
    },
  )
    .then(async (response) => ({
      status: response.status,
      body: (await response.json()) as {
        ok: boolean;
        data: { status: string; wake_id?: string };
      },
    }))
    .finally(() => {
      chatPostSettled = true;
    });
  const streamedEvents = await streamedEventsPromise;
  const chatStreamDurationMs = performance.now() - chatStartedAt;
  assert.equal(
    chatPostSettled,
    false,
    "Rusty View chat SSE should receive responses text before chat POST completes",
  );
  const chatPost = await chatPostPromise;
  const chatPostDurationMs = performance.now() - chatStartedAt;
  assert.equal(chatPost.status, 202);
  assert.equal(chatPost.body.ok, true);
  assert.equal(chatPost.body.data.status, "accepted");

  console.log(
    JSON.stringify(
      {
        profile: "responses-event-loop-profile",
        sessions: [
          "responses-event-loop-session",
          "responses-chat-stream-session",
        ],
        route: "/v1/debug/sessions/responses-event-loop-session/turn",
        concurrentRoute: "/v1/admin/diagnostics",
        chatRoute: "/v1/chat/sessions/responses-chat-stream-session/messages",
        fakeDelayMs,
        preAdminDelayMs: Math.round(preAdminDelayMs),
        adminDurationMs: Math.round(adminDurationMs),
        turnDurationMs: Math.round(turnDurationMs),
        chatStreamDurationMs: Math.round(chatStreamDurationMs),
        chatPostDurationMs: Math.round(chatPostDurationMs),
        streamedEvents: streamedEvents.map((event) => event.kind),
        wakeId: turn.wakeId,
        chatWakeId: chatPost.body.data.wake_id,
      },
      null,
      2,
    ),
  );
} finally {
  if (originalFakeDelay === undefined) {
    delete process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS;
  } else {
    process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS = originalFakeDelay;
  }
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

function writeRuntimeConfig(rootDir: string): void {
  const configDir = join(rootDir, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [
          {
            profileId: "responses-event-loop-profile",
            implementationId: "responses-event-loop-brain",
          },
        ],
        sessions: [
          {
            sessionId: "responses-event-loop-session",
            agentId: "responses-event-loop-agent",
            profileId: "responses-event-loop-profile",
            kind: "full",
          },
          {
            sessionId: "responses-chat-stream-session",
            agentId: "responses-chat-stream-agent",
            profileId: "responses-event-loop-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "responses-event-loop-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-event-loop-profile",
        displayName: "Responses Event Loop Smoke",
        modelConfig: {
          provider: "openai",
          modelName: "gpt-5",
          api: "responses",
        },
        brain: {
          module: "openai-responses",
          strategy: "replay",
        },
        toolPolicy: {
          requestedTools: [],
        },
      },
      null,
      2,
    ),
  );
}

function sleep(delayMs: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, delayMs));
}

interface SseEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
  payload?: Record<string, unknown>;
}

async function collectSseEventsUntil(
  response: Response,
  done: (events: SseEvent[]) => boolean,
  controller: AbortController,
): Promise<SseEvent[]> {
  assert.ok(response.body, "SSE response should have a body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  const deadline = Date.now() + 5_000;
  try {
    while (!done(parseSseEvents(text)) && Date.now() < deadline) {
      const remaining = Math.max(deadline - Date.now(), 1);
      const read = await Promise.race([
        reader.read(),
        new Promise<ReadableStreamReadResult<Uint8Array>>((resolve) =>
          setTimeout(
            () => resolve({ done: true, value: undefined }),
            remaining,
          ),
        ),
      ]);
      if (read.done) break;
      text += decoder.decode(read.value, { stream: true });
    }
  } finally {
    controller.abort();
    await reader.cancel().catch(() => undefined);
  }
  const events = parseSseEvents(text);
  assert.ok(
    done(events),
    `SSE stream did not reach expected condition; received ${events
      .map((event) => event.kind)
      .join(", ")}`,
  );
  return events;
}

function parseSseEvents(text: string): SseEvent[] {
  return text
    .split("\n\n")
    .map((block) => block.trim())
    .filter((block) => block.includes("data: "))
    .map((block) => {
      const data = block
        .split("\n")
        .find((line) => line.startsWith("data: "))
        ?.slice("data: ".length);
      assert.ok(data, "SSE event should include data");
      return JSON.parse(data) as SseEvent;
    });
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const discovered = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(discovered);
      });
    });
  });
}
