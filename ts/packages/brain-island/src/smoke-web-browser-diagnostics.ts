import assert from "node:assert/strict";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import {
  BrowserSessionManager,
  buildWebBrowserDiagnostics,
  cleanupWebBrowserCapabilities,
  defaultToolRegistry,
} from "./index.js";
import type {
  BrowserLaunchResult,
  BrowserLauncher,
  BrowserProcessHandle,
  CdpConnection,
} from "./index.js";

class FakeProcess implements BrowserProcessHandle {
  killed = false;

  kill(): boolean {
    this.killed = true;
    return true;
  }
}

class FakeCdp implements CdpConnection {
  async call(): Promise<unknown> {
    return {};
  }

  close(): void {
    return undefined;
  }
}

const launcher: BrowserLauncher = {
  async launch(): Promise<BrowserLaunchResult> {
    return {
      process: new FakeProcess(),
      cdp: new FakeCdp(),
      userDataDir: "/tmp/rusty-crew-diagnostics-browser",
    };
  },
};

let nowMs = Date.parse("2026-06-20T00:00:00.000Z");
const manager = new BrowserSessionManager({
  launcher,
  now: () => new Date(nowMs),
  limits: { idleTimeoutMs: 1_000 },
});
await manager.open({
  sessionId: "session-diag" as SessionId,
  agentId: "agent-diag" as AgentId,
  profileId: "profile-diag" as ProfileId,
});

const diagnostics = buildWebBrowserDiagnostics({
  env: {
    RUSTY_CREW_SEARXNG_URL: "https://secret.example/search?token=hidden",
    RUSTY_CREW_ALLOW_PRIVATE_NET: "1",
  },
  manager,
  browserBinaryPath: "/opt/chromium/chrome",
  browserBinaryAvailable: false,
  screenshotStoreConfigured: false,
});

assert.equal(diagnostics.web.provider, "searxng");
assert.equal(diagnostics.web.searxngHost, "secret.example");
assert.equal(diagnostics.web.allowPrivateNet, true);
assert.equal(diagnostics.browser.binaryPathLabel, "chrome");
assert.equal(diagnostics.browser.binaryAvailability, "unavailable");
assert.equal(diagnostics.browser.manager?.activeSessions, 1);

const inventory = defaultToolRegistry.buildInventory({
  requestedToolsets: ["web_research", "browser", "browser_vision"],
  ...diagnostics.inventoryRequest,
});
assert.equal(
  inventory.items.find((item) => item.name === "browser_navigate")?.status,
  "resource_denied",
);
assert.equal(
  inventory.items.find((item) => item.name === "browser_vision")?.reasons[0],
  "browser screenshot artifact store is not configured",
);

nowMs += 2_000;
const cleanup = await cleanupWebBrowserCapabilities({
  manager,
  now: new Date(nowMs),
});
assert.equal(cleanup.closed, 1);
assert.equal(cleanup.reasons.idle_timeout, 1);
assert.equal(manager.diagnostics().activeSessions, 0);

console.log(
  JSON.stringify(
    {
      provider: diagnostics.web.provider,
      searxngHost: diagnostics.web.searxngHost,
      browserAvailability: diagnostics.browser.binaryAvailability,
      deniedTools: diagnostics.inventoryRequest.resourceDeniedTools,
      cleanup,
    },
    null,
    2,
  ),
);
