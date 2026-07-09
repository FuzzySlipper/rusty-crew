import assert from "node:assert/strict";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import {
  BrowserSessionManager,
  closeAllServiceBrowserSessionsForLifecycle,
  closeServiceBrowserSessionForLifecycle,
  createServiceBrowserResources,
} from "../src/index.js";
import type {
  BrowserLaunchInput,
  BrowserLaunchResult,
  BrowserLauncher,
  BrowserProcessHandle,
  CdpConnection,
} from "../src/index.js";

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

  async call(): Promise<unknown> {
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
    const process = new FakeProcess(20_000 + launched.length);
    const cdp = new FakeCdp();
    processes.push(process);
    cdps.push(cdp);
    return {
      process,
      cdp,
      userDataDir: `/tmp/rusty-crew-browser-service-lifecycle-${launched.length}`,
    };
  },
};

const manager = new BrowserSessionManager({ launcher });
const resources = createServiceBrowserResources({ manager });

await manager.open(openInput("alpha", "agent-a", "profile-a"));
await manager.open(openInput("beta", "agent-b", "profile-b"));
assert.equal(resources.manager.diagnostics().activeSessions, 2);

const archived = await closeServiceBrowserSessionForLifecycle({
  resources,
  sessionId: "alpha" as SessionId,
  reason: "session_archived",
});
assert.equal(archived.closed, true);
assert.equal(archived.reason, "session_archived");
assert.equal(resources.manager.diagnostics().activeSessions, 1);
assert.equal(processes[0]?.killed, true);
assert.equal(cdps[0]?.closed, true);

const missing = await closeServiceBrowserSessionForLifecycle({
  resources,
  sessionId: "missing" as SessionId,
  reason: "session_archived",
});
assert.equal(missing.closed, false);
assert.equal(resources.manager.diagnostics().activeSessions, 1);

const shutdown = await closeAllServiceBrowserSessionsForLifecycle({
  resources,
  reason: "service_shutdown",
});
assert.equal(shutdown.closed, 1);
assert.equal(shutdown.reasons.service_shutdown, 1);
assert.equal(resources.manager.diagnostics().activeSessions, 0);
assert.equal(processes[1]?.killed, true);
assert.equal(cdps[1]?.closed, true);

console.log(
  JSON.stringify(
    {
      launches: launched.length,
      archived,
      shutdown,
      killed: processes.filter((process) => process.killed).length,
      closedCdp: cdps.filter((cdp) => cdp.closed).length,
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
