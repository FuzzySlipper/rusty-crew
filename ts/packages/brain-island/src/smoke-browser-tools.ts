import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  BrowserSessionManager,
  browserBackTool,
  browserClickTool,
  browserConsoleTool,
  browserNavigateTool,
  browserPressTool,
  browserScrollTool,
  browserSnapshotTool,
  browserTypeTool,
  browserVisionTool,
  createBrowserToolResolver,
  MemoryBrowserScreenshotStore,
  resolveToolSession,
} from "./index.js";
import type {
  BrainWakeInput,
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
  calls: Array<{ method: string; params?: Record<string, unknown> }> = [];

  async call(
    method: string,
    params?: Record<string, unknown>,
  ): Promise<unknown> {
    this.calls.push({ method, params });
    if (
      method === "Page.navigate" ||
      method === "Page.loadEventFired" ||
      method === "Input.dispatchKeyEvent"
    ) {
      return {};
    }
    if (method === "Page.captureScreenshot") {
      return { data: Buffer.from("fakepng").toString("base64") };
    }
    if (method === "Accessibility.getFullAXTree") {
      return {
        nodes: [
          { role: { value: "document" }, name: { value: "Example Page" } },
          { role: { value: "button" }, name: { value: "Save" } },
        ],
      };
    }
    if (method === "Runtime.evaluate") {
      const expression = params?.expression;
      if (expression === "document.title") {
        return { result: { value: "Example Page" } };
      }
      if (expression === "location.href") {
        return { result: { value: "https://example.com/page" } };
      }
      if (expression === "document.readyState") {
        return { result: { value: "complete" } };
      }
      if (
        typeof expression === "string" &&
        expression.includes("querySelectorAll")
      ) {
        return {
          result: {
            value: [
              {
                selector: '[data-rusty-crew-ref="e0"]',
                role: "button",
                name: "Save",
              },
              {
                selector: '[data-rusty-crew-ref="e1"]',
                role: "link",
                name: "Docs",
              },
            ],
          },
        };
      }
      if (typeof expression === "string") {
        return { result: { value: null } };
      }
    }
    throw new Error(`unexpected CDP call ${method}`);
  }

  close(): void {
    return undefined;
  }
}

const cdp = new FakeCdp();
const launcher: BrowserLauncher = {
  async launch(): Promise<BrowserLaunchResult> {
    return {
      process: new FakeProcess(),
      cdp,
      userDataDir: "/tmp/rusty-crew-browser-tools",
      pageWebSocketUrl: "ws://127.0.0.1/browser-tools",
    };
  },
};
const manager = new BrowserSessionManager({ launcher });
const screenshotStore = new MemoryBrowserScreenshotStore();
const context = {
  manager,
  session: wakeWithBrowserTools().state.session,
  resolveHostAddresses: async () => [
    { address: "93.184.216.34", family: 4 as const },
  ],
  screenshotStore,
};

const navigation = await browserNavigateTool(context).execute("navigate", {
  url: "https://example.com/page",
});
assert.equal(navigation.details.action, "navigate");
assert.equal(navigation.details.url, "https://example.com/page");

const firstSnapshot = await browserSnapshotTool(context).execute(
  "snapshot",
  {},
);
assert.equal(firstSnapshot.details.title, "Example Page");
assert.equal(firstSnapshot.details.url, "https://example.com/page");
assert.equal(firstSnapshot.details.generation, 2);
assert.equal(firstSnapshot.details.refCount, 2);
const snapshotText = firstSnapshot.content.find((part) => part.type === "text");
assert.match(snapshotText?.text ?? "", /interactive elements/);
assert.equal(
  manager.resolveRef("session-browser-tools" as SessionId, 2, "@e0")?.target,
  '[data-rusty-crew-ref="e0"]',
);

const click = await browserClickTool(context).execute("click", { ref: "@e0" });
assert.equal(click.details.action, "click");
assert.equal(
  manager.resolveRef("session-browser-tools" as SessionId, 2, "@e0"),
  undefined,
);

const secondSnapshot = await browserSnapshotTool(context).execute(
  "snapshot-again",
  {},
);
assert.equal(secondSnapshot.details.generation, 4);

const type = await browserTypeTool(context).execute("type", {
  ref: "@e1",
  text: "hello",
});
assert.equal(type.details.action, "type");
assert.equal(type.details.textLength, 5);

const scroll = await browserScrollTool(context).execute("scroll", {
  direction: "down",
});
assert.equal(scroll.details.action, "scroll");

const back = await browserBackTool(context).execute("back", {});
assert.equal(back.details.action, "back");

const press = await browserPressTool(context).execute("press", {
  key: "Enter",
});
assert.equal(press.details.action, "press");

const consoleResult = await browserConsoleTool(context).execute("console", {
  expression: "document.readyState",
});
assert.equal(consoleResult.details.expression, "document.readyState");
assert.equal(consoleResult.details.result, "complete");
assert.equal(manager.diagnostics().sessions[0]?.consoleCount, 1);

const visionResult = await browserVisionTool(context).execute("vision", {});
assert.equal(visionResult.details.ok, true);
assert.equal(visionResult.details.artifact.mediaType, "image/png");
assert.equal(visionResult.details.artifact.byteLength, 7);
assert.equal(
  screenshotStore.get(visionResult.details.artifact.ref)?.byteLength,
  7,
);
const visionText = visionResult.content.find((part) => part.type === "text");
assert.equal(visionText?.text.includes("ZmFrZXBuZw=="), false);

const selection = resolveToolSession({
  wake: wakeWithBrowserTools(),
  resolveTools: createBrowserToolResolver({ manager }),
});
assert.deepEqual(
  selection.tools.map((tool) => tool.name),
  ["browser_snapshot", "browser_console"],
);
assert.equal(selection.items[0]?.status, "callable");
assert.equal(selection.items[1]?.status, "callable");

console.log(
  JSON.stringify(
    {
      generation: secondSnapshot.details.generation,
      refs: secondSnapshot.details.refs.map((ref) => ref.ref),
      actionCalls: cdp.calls.filter((call) =>
        ["Page.navigate", "Input.dispatchKeyEvent"].includes(call.method),
      ).length,
      screenshotRef: visionResult.details.artifact.ref,
      consoleExpression: consoleResult.details.expression,
      selectedTools: selection.tools.map((tool) => tool.name),
    },
    null,
    2,
  ),
);

function wakeWithBrowserTools(): BrainWakeInput {
  return {
    wakeId: "wake-browser-tools",
    sessionId: "session-browser-tools" as SessionId,
    state: {
      session: {
        handle: 3 as SessionHandle,
        sessionId: "session-browser-tools" as SessionId,
        agentId: "agent-browser-tools" as AgentId,
        profileId: "profile-browser-tools" as ProfileId,
        kind: "full",
        status: "active",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00.000Z",
        lastActiveAt: "2026-06-20T00:00:00.000Z",
        resourceLimits: {},
        toolProfile: {
          tools: [
            {
              name: "browser_snapshot",
              description:
                "Return a bounded accessibility and interactive-element snapshot.",
            },
            {
              name: "browser_console",
              description:
                "Run one bounded page diagnostic expression and return JSON.",
            },
          ],
        },
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 60_000,
        maxQueuedMessages: 8,
      },
    },
    systemPrompt: "",
    roleAssembly: {
      instructions: "Use browser observation tools.",
    },
  };
}
