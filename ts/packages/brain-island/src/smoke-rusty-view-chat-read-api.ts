import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId } from "@rusty-crew/contracts";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-view-chat-read-api-"));
const port = await openPort();
const token = "rusty-view-chat-token";
writeRuntimeConfig(root);
let host = await startHost();

try {
  const unauthorized = await get("/v1/chat/sessions");
  assert.equal(unauthorized.status, 401);

  const page = await get("/v1/chat/sessions", token);
  assert.equal(page.status, 200);
  assert.equal(page.body.ok, true);
  assert.equal(page.body.data.total, 1);
  assert.equal(page.body.data.items[0]?.session_id, "chat-session");
  assert.equal(typeof page.body.data.items[0]?.latest_cursor, "string");

  await host.bridge.routeAgentMessage(
    "human-operator" as AgentId,
    "chat-agent" as AgentId,
    "hello from Rusty View",
    "chat-smoke-1",
  );

  const opened = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(opened.status, 200);
  assert.equal(opened.body.data.session.session_id, "chat-session");
  assert.deepEqual(
    opened.body.data.events.map((event: { kind: string }) => event.kind),
    ["session_snapshot", "message_created"],
  );
  assert.equal(
    opened.body.data.events[1]?.payload.body,
    "hello from Rusty View",
  );
  assert.equal(opened.body.data.events[1]?.payload.role, "user");

  const missing = await get("/v1/chat/sessions/missing-session", token);
  assert.equal(missing.status, 404);
  assert.equal(missing.body.error.reason_code, "chat_session_not_found");

  const streamAbort = new AbortController();
  const streamResponse = await fetch(
    `http://127.0.0.1:${port}/v1/chat/sessions/chat-session/stream`,
    {
      headers: { authorization: `Bearer ${token}` },
      signal: streamAbort.signal,
    },
  );
  assert.equal(streamResponse.status, 200);
  assert.ok(
    streamResponse.headers.get("content-type")?.includes("text/event-stream"),
  );
  const streamedEventsPromise = collectSseEvents(streamResponse, 5, streamAbort);

  const sent = await post(
    "/v1/chat/sessions/chat-session/messages",
    token,
    {
      actor: { id: "human-operator", kind: "human" },
      body: "please answer from the chat endpoint",
      client_message_id: "client-message-1",
    },
    { "Idempotency-Key": "chat-send-1" },
  );
  assert.equal(sent.status, 202);
  assert.equal(sent.body.ok, true);
  assert.equal(sent.body.data.status, "accepted");
  assert.equal(sent.body.data.message_id, "client-message-1");
  assert.equal(typeof sent.body.data.wake_id, "string");

  const streamedEvents = await streamedEventsPromise;
  assert.ok(
    streamedEvents.some((event) => event.kind === "message_created"),
    "active stream should receive the submitted message event",
  );
  assert.ok(
    streamedEvents.some((event) => event.kind === "assistant_turn_finished"),
    "active stream should receive assistant turn completion lifecycle",
  );

  const eventsAfterSnapshot = await get(
    `/v1/chat/sessions/chat-session/events?cursor=${encodeURIComponent(
      "chat-session:0",
    )}`,
    token,
  );
  assert.equal(eventsAfterSnapshot.status, 200);
  assert.ok(eventsAfterSnapshot.body.data.items.length >= 4);
  assert.equal(eventsAfterSnapshot.body.data.items[0]?.kind, "message_created");

  const replayCursor = streamedEvents[1]?.event_id;
  assert.equal(typeof replayCursor, "string");
  const replay = await getSseOnce(
    `/v1/chat/sessions/chat-session/stream?once=true&cursor=${encodeURIComponent(
      replayCursor,
    )}`,
    token,
  );
  assert.ok(
    replay.every((event) => event.sequence_id > streamedEvents[1].sequence_id),
    "cursor replay should only return missed events",
  );

  const duplicate = await post(
    "/v1/chat/sessions/chat-session/messages",
    token,
    {
      actor: { id: "human-operator", kind: "human" },
      body: "this duplicate should not dispatch",
      client_message_id: "client-message-1",
    },
    { "Idempotency-Key": "chat-send-1" },
  );
  assert.equal(duplicate.status, 202);
  assert.equal(duplicate.body.data.status, "duplicate");
  assert.equal(duplicate.body.data.wake_id, sent.body.data.wake_id);

  const empty = await post("/v1/chat/sessions/chat-session/messages", token, {
    actor: { id: "human-operator", kind: "human" },
    body: " ",
  });
  assert.equal(empty.status, 400);
  assert.equal(empty.body.error.reason_code, "empty_chat_message");

  const missingSend = await post("/v1/chat/sessions/missing/messages", token, {
    actor: { id: "human-operator", kind: "human" },
    body: "hello",
  });
  assert.equal(missingSend.status, 404);

  const registry = await get("/v1/chat/commands", token);
  assert.equal(registry.status, 200);
  assert.deepEqual(
    registry.body.data.commands.map((command: { name: string }) => command.name),
    ["help", "status", "session", "new", "reload-mcp"],
  );

  const statusCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/status",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(statusCommand.status, 200);
  assert.equal(statusCommand.body.data.status, "completed");
  assert.equal(statusCommand.body.data.command_name, "status");

  const unknownCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/definitely-not-real",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(unknownCommand.status, 409);
  assert.equal(unknownCommand.body.data.status, "rejected");
  assert.equal(unknownCommand.body.data.reason_code, "unknown_command");

  const reloadCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/reload-mcp",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(reloadCommand.status, 409);
  assert.equal(reloadCommand.body.data.command_name, "reload-mcp");
  assert.equal(reloadCommand.body.data.status, "failed");

  await host.stop();
  host = await startHost();

  const restarted = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(restarted.status, 200);
  assert.equal(restarted.body.data.events[0]?.kind, "session_snapshot");
  assert.equal(restarted.body.data.session.session_id, "chat-session");

  console.log(
    JSON.stringify(
      {
        sessions: page.body.data.total,
        openedEvents: opened.body.data.events.length,
        sendStatus: sent.body.data.status,
        duplicateStatus: duplicate.body.data.status,
        streamedEvents: streamedEvents.map((event) => event.kind),
        restartEvent: restarted.body.data.events[0]?.kind,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

async function startHost() {
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "0",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "0",
    },
  });
}

async function get(path: string, bearer?: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: bearer ? { authorization: `Bearer ${bearer}` } : undefined,
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function post(
  path: string,
  bearer: string | undefined,
  body: unknown,
  extraHeaders: Record<string, string> = {},
) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
      "content-type": "application/json",
      ...extraHeaders,
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: (await response.json()) as any,
  };
}

async function getSseOnce(path: string, bearer: string): Promise<SseEvent[]> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: { authorization: `Bearer ${bearer}` },
  });
  assert.equal(response.status, 200);
  assert.ok(response.headers.get("content-type")?.includes("text/event-stream"));
  const text = await response.text();
  return parseSseEvents(text);
}

async function collectSseEvents(
  response: Response,
  count: number,
  controller: AbortController,
): Promise<SseEvent[]> {
  assert.ok(response.body, "SSE response should have a body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  const deadline = Date.now() + 5_000;
  try {
    while (parseSseEvents(text).length < count && Date.now() < deadline) {
      const remaining = Math.max(deadline - Date.now(), 1);
      const read = await Promise.race([
        reader.read(),
        new Promise<ReadableStreamReadResult<Uint8Array>>((resolve) =>
          setTimeout(() => resolve({ done: true, value: undefined }), remaining),
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
    events.length >= count,
    `expected at least ${count} SSE event(s), received ${events.length}`,
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

interface SseEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
}

function writeRuntimeConfig(dataRoot: string): void {
  const configDir = join(dataRoot, "config");
  const profilesDir = join(configDir, "profiles");
  const skillsDir = join(configDir, "skills");
  mkdirSync(profilesDir, { recursive: true });
  mkdirSync(skillsDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        skillsDir,
        brains: [{ profileId: "chat-profile" }],
        sessions: [
          {
            sessionId: "chat-session",
            agentId: "chat-agent",
            profileId: "chat-profile",
            kind: "full",
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "chat-profile.json"),
    JSON.stringify(
      {
        profileId: "chat-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        prompt: {
          system: "Chat profile system prompt.",
          instructions: ["Answer concisely."],
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
      const open = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(open);
      });
    });
  });
}
