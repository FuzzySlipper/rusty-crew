import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createDebugApiClient } from "./debug-api-client.js";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const tmpRoot = join(tmpdir(), "rusty-crew");
mkdirSync(tmpRoot, { recursive: true });

const root = mkdtempSync(join(tmpRoot, "responses-capacity-"));
const port = await openPort();
const sessionCount = positiveInteger(
  process.env.RUSTY_CREW_OPENAI_RESPONSES_CAPACITY_SESSIONS,
  4,
);
const fakeDelayMs = positiveInteger(
  process.env.RUSTY_CREW_OPENAI_RESPONSES_CAPACITY_FAKE_DELAY_MS,
  600,
);
const originalFakeDelay = process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS;
process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS = String(fakeDelayMs);

writeRuntimeConfig(root, sessionCount);
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

interface ActiveStream {
  sessionId: string;
  controller: AbortController;
  reader: ReadableStreamDefaultReader<Uint8Array>;
}

const activeStreams: ActiveStream[] = [];

try {
  const client = createDebugApiClient({
    baseUrl: `http://127.0.0.1:${port}`,
    retries: 0,
    timeoutMs: 5_000,
  });
  const sessionIds = Array.from(
    { length: sessionCount },
    (_, index) => `responses-capacity-session-${index + 1}`,
  );

  const streamRecords = await Promise.all(
    sessionIds.map((sessionId) => openSessionStream(port, sessionId)),
  );
  const startedAt = performance.now();
  const firstDeltaFlags = new Map<string, boolean>(
    sessionIds.map((sessionId) => [sessionId, false]),
  );
  const firstDeltaPromises = streamRecords.map((record) =>
    collectFirstDelta(record, startedAt).then((result) => {
      firstDeltaFlags.set(result.sessionId, true);
      return result;
    }),
  );

  const postPromises = sessionIds.map((sessionId, index) =>
    postChatMessage(port, sessionId, index + 1, startedAt),
  );

  await sleep(25);
  const adminStartedAt = performance.now();
  const diagnostics = await client.diagnostics();
  const adminDurationMs = performance.now() - adminStartedAt;
  assert.equal(typeof diagnostics.overview.generatedAt, "string");
  assert.ok(
    adminDurationMs < Math.max(500, fakeDelayMs),
    `admin diagnostics was delayed ${Math.round(
      adminDurationMs,
    )}ms during ${sessionCount} concurrent fake responses wakes`,
  );

  const firstDeltas = await Promise.all(firstDeltaPromises);
  const posts = await Promise.all(postPromises);
  const totalDurationMs = performance.now() - startedAt;

  for (const post of posts) {
    assert.equal(post.status, 202, JSON.stringify(post.body));
    assert.equal(post.body.ok, true, JSON.stringify(post.body));
    assert.equal(post.body.data.status, "accepted");
  }
  for (const sessionId of sessionIds) {
    assert.equal(
      firstDeltaFlags.get(sessionId),
      true,
      `session ${sessionId} did not emit assistant_text_delta`,
    );
  }

  const serializedEquivalentMs = fakeDelayMs * sessionCount;
  const parallelismEstimate = serializedEquivalentMs / totalDurationMs;
  const likelySerialized = totalDurationMs > serializedEquivalentMs * 0.75;
  assert.equal(
    likelySerialized,
    false,
    `${sessionCount} fake responses wakes looked serialized: ${Math.round(
      totalDurationMs,
    )}ms total for ${serializedEquivalentMs}ms serialized equivalent`,
  );

  console.log(
    JSON.stringify(
      {
        profile: "responses-capacity-profile",
        route: "/v1/chat/sessions/:sessionId/messages",
        streamRoute: "/v1/chat/sessions/:sessionId/stream",
        concurrentRoute: "/v1/admin/diagnostics",
        sessionCount,
        fakeDelayMs,
        adminDurationMs: Math.round(adminDurationMs),
        firstDeltaLatencyMs: summarize(
          firstDeltas.map((result) => result.firstDeltaLatencyMs),
        ),
        postCompletionLatencyMs: summarize(
          posts.map((result) => result.postCompletionLatencyMs),
        ),
        totalDurationMs: Math.round(totalDurationMs),
        serializedEquivalentMs,
        parallelismEstimate: Number(parallelismEstimate.toFixed(2)),
        likelySerialized,
        wakeIds: posts.map((post) => post.body.data.wake_id),
      },
      null,
      2,
    ),
  );
} finally {
  for (const record of activeStreams) {
    record.controller.abort();
    await record.reader.cancel().catch(() => undefined);
  }
  if (originalFakeDelay === undefined) {
    delete process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS;
  } else {
    process.env.RUSTY_CREW_OPENAI_RESPONSES_FAKE_DELAY_MS = originalFakeDelay;
  }
  await host.stop().catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

async function openSessionStream(
  hostPort: number,
  sessionId: string,
): Promise<ActiveStream> {
  const controller = new AbortController();
  const response = await fetch(
    `http://127.0.0.1:${hostPort}/v1/chat/sessions/${sessionId}/stream`,
    { signal: controller.signal },
  );
  assert.equal(response.status, 200);
  assert.equal(
    response.headers.get("content-type")?.includes("text/event-stream"),
    true,
  );
  assert.ok(response.body, "SSE response should include a body");
  const record = {
    sessionId,
    controller,
    reader: response.body.getReader(),
  };
  activeStreams.push(record);
  return record;
}

async function collectFirstDelta(
  stream: ActiveStream,
  startedAt: number,
): Promise<{ sessionId: string; firstDeltaLatencyMs: number }> {
  const decoder = new TextDecoder();
  let text = "";
  const deadline =
    Date.now() + Math.max(10_000, fakeDelayMs * sessionCount * 2);
  while (Date.now() < deadline) {
    const remaining = Math.max(deadline - Date.now(), 1);
    const read = await Promise.race([
      stream.reader.read(),
      new Promise<ReadableStreamReadResult<Uint8Array>>((resolve) =>
        setTimeout(() => resolve({ done: true, value: undefined }), remaining),
      ),
    ]);
    if (read.done) break;
    text += decoder.decode(read.value, { stream: true });
    if (
      parseSseEvents(text).some(
        (event) => event.kind === "assistant_text_delta",
      )
    ) {
      return {
        sessionId: stream.sessionId,
        firstDeltaLatencyMs: performance.now() - startedAt,
      };
    }
  }
  assert.fail(
    `SSE stream for ${stream.sessionId} did not emit assistant_text_delta; received ${parseSseEvents(
      text,
    )
      .map((event) => event.kind)
      .join(", ")}`,
  );
}

async function postChatMessage(
  hostPort: number,
  sessionId: string,
  index: number,
  startedAt: number,
): Promise<{
  sessionId: string;
  status: number;
  postCompletionLatencyMs: number;
  body: {
    ok: boolean;
    data: { status: string; wake_id?: string };
  };
}> {
  const response = await fetch(
    `http://127.0.0.1:${hostPort}/v1/chat/sessions/${sessionId}/messages`,
    {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "idempotency-key": `responses-capacity-${index}`,
      },
      body: JSON.stringify({
        actor: { id: "capacity-operator", kind: "human" },
        body: `Run concurrent fake Responses wake ${index}.`,
        client_message_id: `responses-capacity-message-${index}`,
      }),
    },
  );
  return {
    sessionId,
    status: response.status,
    postCompletionLatencyMs: performance.now() - startedAt,
    body: (await response.json()) as {
      ok: boolean;
      data: { status: string; wake_id?: string };
    },
  };
}

interface SseEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
  payload?: Record<string, unknown>;
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

function writeRuntimeConfig(rootDir: string, count: number): void {
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
            profileId: "responses-capacity-profile",
            implementationId: "responses-capacity-brain",
          },
        ],
        sessions: Array.from({ length: count }, (_, index) => ({
          sessionId: `responses-capacity-session-${index + 1}`,
          agentId: `responses-capacity-agent-${index + 1}`,
          profileId: "responses-capacity-profile",
          kind: "full",
        })),
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "responses-capacity-profile.json"),
    JSON.stringify(
      {
        profileId: "responses-capacity-profile",
        displayName: "Responses Capacity Smoke",
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

function summarize(values: number[]): {
  min: number;
  max: number;
  mean: number;
} {
  const rounded = values.map((value) => Math.round(value));
  return {
    min: Math.min(...rounded),
    max: Math.max(...rounded),
    mean: Math.round(
      rounded.reduce((total, value) => total + value, 0) / rounded.length,
    ),
  };
}

function positiveInteger(raw: string | undefined, fallback: number): number {
  if (raw === undefined) return fallback;
  const parsed = Number(raw);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : fallback;
}

function sleep(delayMs: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, delayMs));
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
