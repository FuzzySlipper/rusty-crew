import assert from "node:assert/strict";
import { createServer } from "node:http";

import type { SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

const server = createServer((_request, response) => {
  response.writeHead(200, {
    "content-type": "text/event-stream",
    "cache-control": "no-cache",
  });
  response.write(
    ": upstream accepted request but produced no provider event\n\n",
  );
});
await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
const address = server.address();
assert(address !== null && typeof address !== "string");

const bridge = await loadNativeBridge();
const wakeId = `chat-completions-silent-upstream-${Date.now()}`;
const startedAt = Date.now();
try {
  await bridge.startBrainRun({
    moduleId: "chat-completions",
    providerInput: {
      wakeId,
      sessionId: "chat-completions-silent-upstream-session" as SessionId,
      messages: [{ role: "user", content: "Produce one response." }],
      config: {
        model: "silent-upstream",
        providerRequestTimeoutMs: 200,
        wakeTimeoutMs: 2_000,
        maxOutputTokens: 16,
      },
      client: {
        mode: "live",
        baseUrl: `http://127.0.0.1:${address.port}/v1`,
      },
    },
  });

  let terminal: Awaited<ReturnType<typeof bridge.drainBrainRun>> | undefined;
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const drained = await bridge.drainBrainRun({
      moduleId: "chat-completions",
      wakeId,
      maxItems: 64,
    });
    if (drained.terminal) {
      terminal = drained;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 25));
  }
  assert.ok(terminal, "silent upstream must produce a terminal drain");
  assert.match(terminal.error ?? "", /provider request timeout/);
  const elapsedMs = Date.now() - startedAt;
  assert.ok(elapsedMs >= 150, `timeout fired too early at ${elapsedMs}ms`);
  assert.ok(elapsedMs < 2_000, `host wake ceiling won at ${elapsedMs}ms`);
  console.log(
    JSON.stringify({
      wakeId,
      elapsedMs,
      terminal: terminal.terminal,
      error: terminal.error,
      providerTimeoutMs: 200,
      hostWakeTimeoutMs: 2_000,
    }),
  );
} finally {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}
