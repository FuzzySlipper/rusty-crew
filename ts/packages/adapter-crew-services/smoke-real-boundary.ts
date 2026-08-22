import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { promisify } from "node:util";
import { execFile } from "node:child_process";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { CrewServicesDirectBrainAdapter } from "./src/adapter.js";
import { CrewServicesClient } from "./src/client.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-fabric-real-"));
const port = await freePort();
const baseUrl = `http://127.0.0.1:${port}`;
const binary = await crewServicesBinary(root);
const server = spawn(
  binary,
  ["-db", join(root, "fabric.db"), "-listen", `127.0.0.1:${port}`],
  { cwd: process.env.CREW_SERVICES_DIR ?? "/home/dev/crew-services", stdio: "pipe" },
);
const native = await loadNativeBridge();
let engine: number | undefined;

try {
  await ready(baseUrl);
  engine = await native.initializeEngine({ engineDataDir: join(root, "engine"), clock: "system", defaultTurnBudget: 3, defaultIdleTimeoutMs: 1_000 });
  const alpha = "fabric-alpha-session" as SessionId;
  const beta = "fabric-beta-session" as SessionId;
  await Promise.all([
    native.createSession({ sessionId: alpha, agentId: "fabric-alpha" as AgentId, profileId: "fabric" as ProfileId, kind: "full" }),
    native.createSession({ sessionId: beta, agentId: "fabric-beta" as AgentId, profileId: "fabric" as ProfileId, kind: "full" }),
  ]);
  await Promise.all([
    native.putAgentRoute({ routeKey: "fabric-alpha", label: "Fabric alpha", enabled: true, updatedAt: new Date().toISOString(), target: { type: "direct_brain", agentId: "fabric-alpha", sessionId: alpha } }),
    native.putAgentRoute({ routeKey: "fabric-beta", label: "Fabric beta", enabled: true, updatedAt: new Date().toISOString(), target: { type: "direct_brain", agentId: "fabric-beta", sessionId: beta } }),
  ]);
  const fabric = new CrewServicesClient(baseUrl);
  const adapter = new CrewServicesDirectBrainAdapter(fabric, native, {
    adapterId: "rusty-crew-fabric-smoke", instanceId: "smoke-instance", pollMs: 60_000,
    bindings: [
      { alias: "alpha", routeKey: "fabric-alpha", routeRevision: 1 },
      { alias: "beta", routeKey: "fabric-beta", routeRevision: 1 },
    ],
  });
  await adapter.start();
  assert.equal(adapter.available(alpha), true);
  assert.equal(adapter.available(beta), true);
  const accepted = await adapter.sendFromSession({ sessionId: alpha, toolCallId: "ordinary-1", recipientAlias: "beta", body: "ordinary real-boundary message" });
  const replay = await adapter.sendFromSession({ sessionId: alpha, toolCallId: "ordinary-1", recipientAlias: "beta", body: "ordinary real-boundary message" });
  assert.equal(accepted.messageId, replay.messageId, "fabric retry is one immutable message");
  await adapter.tick();
  const ordinaryDelivery = (await fabric.listDeliveries()).deliveries.find((item) => item.message_id === accepted.messageId);
  assert.ok(ordinaryDelivery, "ordinary message has a fabric delivery");
  const nativeReceipt = await native.getAgentMessageDelivery(`fabric-delivery:${ordinaryDelivery!.delivery_id}`);
  assert.equal(nativeReceipt?.status, "accepted");
  assert.match(nativeReceipt?.request.body ?? "", /Crew message/);
  assert.match(nativeReceipt?.request.body ?? "", /replyToMessageId/);
  const reply = await adapter.sendFromSession({ sessionId: beta, toolCallId: "linked-reply-1", recipientAlias: "alpha", body: "one linked reply", replyToMessageId: accepted.messageId });
  await adapter.tick();
  const replyDelivery = (await fabric.listDeliveries()).deliveries.find((item) => item.message_id === reply.messageId);
  assert.ok(replyDelivery, "linked reply has a fabric delivery");
  const nativeReply = await native.getAgentMessageDelivery(`fabric-delivery:${replyDelivery!.delivery_id}`);
  assert.match(nativeReply?.request.body ?? "", /terminal reply/);
  const inspection = await fabric.listDeliveries();
  assert.equal(inspection.deliveries.filter((item) => item.message_id === accepted.messageId).length, 1);
  await adapter.stop();
  console.log(JSON.stringify({ ordinary: accepted.messageId, reply: reply.messageId, deliveries: inspection.deliveries.length }));
} finally {
  if (engine !== undefined) await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 }).catch(() => undefined);
  await stopServer(server);
  rmSync(root, { recursive: true, force: true });
}
process.exit(0);

async function crewServicesBinary(root: string): Promise<string> {
  const configured = process.env.CREW_SERVICES_BIN;
  if (configured !== undefined) return configured;
  const binary = join(root, "crew-messaging");
  await promisify(execFile)("go", ["build", "-o", binary, "./cmd/crew-messaging"], {
    cwd: process.env.CREW_SERVICES_DIR ?? "/home/dev/crew-services",
  });
  return binary;
}

async function stopServer(child: ReturnType<typeof spawn>): Promise<void> {
  if (child.exitCode !== null) return;
  const exited = once(child, "exit").then(() => true);
  child.kill("SIGTERM");
  const graceful = await Promise.race([
    exited,
    new Promise<false>((resolve) => setTimeout(() => resolve(false), 2_000)),
  ]);
  if (graceful) return;
  child.kill("SIGKILL");
  await exited;
}

async function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const listener = createServer();
    listener.once("error", reject);
    listener.listen(0, "127.0.0.1", () => {
      const address = listener.address();
      listener.close(() => typeof address === "object" && address !== null ? resolve(address.port) : reject(new Error("no port")));
    });
  });
}
async function ready(url: string): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (await fetch(`${url}/readyz`).then((response) => response.ok).catch(() => false)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`crew-services did not become ready: ${url}`);
}
