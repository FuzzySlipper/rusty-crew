import assert from "node:assert/strict";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import { BrowserSessionManager } from "./index.js";
import type {
  BrowserLaunchInput,
  BrowserLaunchResult,
  BrowserLauncher,
  BrowserProcessHandle,
  CdpConnection,
} from "./index.js";

class FakeProcess implements BrowserProcessHandle {
  killed = false;

  constructor(readonly pid: number) {}

  kill(): boolean {
    this.killed = true;
    return true;
  }
}

class FakeCdp implements CdpConnection {
  closed = false;
  calls: Array<{ method: string; params?: Record<string, unknown> }> = [];

  async call(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<unknown> {
    this.calls.push({ method, params });
    return {};
  }

  close(): void {
    this.closed = true;
  }
}

const launched: BrowserLaunchInput[] = [];
const processes: FakeProcess[] = [];
const cdps: FakeCdp[] = [];
const launcher: BrowserLauncher = {
  async launch(input): Promise<BrowserLaunchResult> {
    launched.push(input);
    const process = new FakeProcess(10_000 + launched.length);
    const cdp = new FakeCdp();
    processes.push(process);
    cdps.push(cdp);
    return {
      process,
      cdp,
      userDataDir: `/tmp/rusty-crew-browser-test-${launched.length}`,
      pageWebSocketUrl: `ws://127.0.0.1/${launched.length}`,
    };
  },
};

let nowMs = Date.parse("2026-06-20T00:00:00.000Z");
const manager = new BrowserSessionManager({
  launcher,
  now: () => new Date(nowMs),
  limits: {
    maxServiceSessions: 2,
    maxSessionsPerAgent: 2,
    idleTimeoutMs: 1_000,
    hardLifetimeMs: 10_000,
    maxRefs: 2,
    consoleRingSize: 2,
  },
});

const alpha = await manager.open(openInput("alpha", "agent-a", "profile-a"));
assert.equal(alpha.state, "ready");
assert.equal(launched.length, 1);

const alphaAgain = await manager.open(
  openInput("alpha", "agent-a", "profile-a"),
);
assert.equal(alphaAgain.sessionId, alpha.sessionId);
assert.equal(launched.length, 1);

const snapshot = manager.storeRefs("alpha" as SessionId, [
  { ref: "@e0", target: "button.primary", role: "button", name: "Save" },
  { ref: "@e1", target: "a.help", role: "link", name: "Help" },
  { ref: "@e2", target: "input.ignored", role: "textbox", name: "Ignored" },
]);
assert.equal(snapshot.generation, 1);
assert.equal(snapshot.refs.length, 2);
assert.equal(
  manager.resolveRef("alpha" as SessionId, 1, "@e0")?.target,
  "button.primary",
);

manager.recordConsole("alpha" as SessionId, "one");
manager.recordConsole("alpha" as SessionId, "two");
manager.recordConsole("alpha" as SessionId, "three");
assert.equal(manager.diagnostics().sessions[0]?.consoleCount, 2);

manager.recordNavigation(
  "alpha" as SessionId,
  "https://example.com",
  "Example",
);
assert.equal(manager.resolveRef("alpha" as SessionId, 1, "@e0"), undefined);
assert.equal(manager.diagnostics().sessions[0]?.generation, 2);
assert.equal(
  manager.diagnostics().sessions[0]?.currentUrl,
  "https://example.com",
);

await manager.open(openInput("beta", "agent-b", "profile-b"));
await assert.rejects(
  () => manager.open(openInput("gamma", "agent-c", "profile-c")),
  /browser session service limit reached/,
);

await manager.close("beta" as SessionId, "manual");
assert.equal(processes[1]?.killed, true);
assert.equal(cdps[1]?.closed, true);

nowMs += 2_000;
const cleanup = await manager.sweep(new Date(nowMs));
assert.equal(cleanup.closed, 1);
assert.equal(cleanup.reasons.idle_timeout, 1);
assert.equal(processes[0]?.killed, true);
assert.equal(cdps[0]?.closed, true);
assert.equal(manager.diagnostics().activeSessions, 0);

await manager.open(openInput("delta", "agent-d", "profile-d"));
await manager.open(openInput("epsilon", "agent-d", "profile-d"));
await manager.closeAllForAgent("agent-d" as AgentId, "agent_closed");
assert.equal(manager.diagnostics().activeSessions, 0);

console.log(
  JSON.stringify(
    {
      launches: launched.length,
      killed: processes.filter((process) => process.killed).length,
      closedCdp: cdps.filter((cdp) => cdp.closed).length,
      cleanup,
    },
    null,
    2,
  ),
);

function openInput(sessionId: string, agentId: string, profileId: string) {
  return {
    sessionId: sessionId as SessionId,
    agentId: agentId as AgentId,
    profileId: profileId as ProfileId,
  };
}
