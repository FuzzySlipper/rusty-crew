import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  BodyState,
  BrainAction,
  BrainImplementationId,
  CoreEvent,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  BrowserSessionManager,
  createBrowserToolResolver,
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  MemoryBrowserScreenshotStore,
  registerBrainImplementationRuntime,
  selectToolProfile,
} from "./index.js";
import type {
  BrowserLaunchResult,
  BrowserLauncher,
  BrowserProcessHandle,
  CdpConnection,
} from "./index.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-production-browser-tools-engine-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

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
      return { data: Buffer.from("fake-production-png").toString("base64") };
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
      userDataDir: "/tmp/rusty-crew-production-browser-tools",
      pageWebSocketUrl: "ws://127.0.0.1/production-browser-tools",
    };
  },
};
const manager = new BrowserSessionManager({ launcher });
const screenshotStore = new MemoryBrowserScreenshotStore();

class BrowserToolCallingFakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly options: PiAgentOptions,
    private readonly outputs: Record<string, string>,
  ) {}

  subscribe(
    listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    this.listener = listener;
    return () => {
      this.listener = undefined;
    };
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    await this.emit({ type: "agent_start" } as PiAgentEvent);
    await this.callTool("browser_navigate", {
      url: "https://example.com/page",
    });
    await this.callTool("browser_snapshot", {});
    await this.callTool("browser_click", { ref: "@e0" });
    await this.callTool("browser_console", {
      expression: "document.readyState",
    });
    await this.callTool("browser_vision", {});
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callTool(name: string, params: Record<string, unknown>) {
    const tool = this.options.initialState?.tools?.find(
      (candidate) => candidate.name === name,
    );
    assert.ok(tool, `${name} should be selected for the wake`);
    await this.emit({
      type: "tool_execution_start",
      toolName: name,
    } as PiAgentEvent);
    try {
      const result = await (tool as AgentTool).execute(`${name}-call`, params);
      this.outputs[name] = result.content
        .flatMap((content) =>
          content.type === "text" && typeof content.text === "string"
            ? [content.text]
            : [],
        )
        .join("");
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: false,
      } as PiAgentEvent);
    } catch (error) {
      await this.emit({
        type: "tool_execution_end",
        toolName: name,
        isError: true,
      } as PiAgentEvent);
      throw error;
    }
  }

  private async emit(event: PiAgentEvent): Promise<void> {
    this.listener?.(event, abortSignal);
  }
}

try {
  const sessionId = "production-browser-tools-session" as SessionId;
  const agentId = "production-browser-tools-agent" as AgentId;
  const profileId = "production-browser-tools-profile" as ProfileId;
  const wakeId = "production-browser-tools-wake";
  const selection = selectToolProfile({
    profileId,
    policy: {
      requestedToolsets: ["browser", "browser_vision"],
    },
  });
  const outputs: Record<string, string> = {};
  const brainEvents = await native.subscribeEvents({
    eventKinds: ["brain_event_observed"],
    sessionId,
  });

  await native.createSession({
    sessionId,
    agentId,
    profileId,
    kind: "full",
  });

  const brain = await registerBrainImplementationRuntime(
    native,
    {
      implementationId: "production-browser-tools" as BrainImplementationId,
      profileId,
      toolProfile: selection.toolProfile,
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createPiAgentBrain({
      createAgent: (options) =>
        new BrowserToolCallingFakeAgent(options, outputs),
      resolveTools: createBrowserToolResolver({
        manager,
        resolveHostAddresses: async () => [
          { address: "93.184.216.34", family: 4 as const },
        ],
        screenshotStore,
      }),
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "production browser tool wake completed",
          },
        },
      ],
    }),
  );

  const request = await native.buildBrainWakeRequest({
    brain,
    sessionId,
    bodyStateJson: encoder.encode(JSON.stringify(bodyState())),
    systemPrompt: "Use the selected production browser tools.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({
        instructions: "Navigate, observe, interact, inspect, and capture.",
      }),
    ),
    wakeId,
  });

  const accepted = await native.wakeBrain(request);
  assert.deepEqual(accepted, { wakeId, accepted: true });
  assert.match(outputs.browser_navigate, /navigated/);
  assert.match(outputs.browser_snapshot, /interactive elements/);
  assert.match(outputs.browser_click, /clicked @e0/);
  assert.match(outputs.browser_console, /complete/);
  assert.match(outputs.browser_vision, /browser-screenshot/);
  assert.equal(manager.diagnostics().sessions[0]?.consoleCount, 1);
  assert.ok(cdp.calls.some((call) => call.method === "Page.captureScreenshot"));
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("tool_call_history"), 10);

  const observedEvents = await native.drainSubscriptionEvents(brainEvents, 20);
  const toolEvents = observedEvents.filter(
    (event): event is Extract<CoreEvent, { type: "brain_event_observed" }> =>
      event.type === "brain_event_observed" &&
      event.event.type.startsWith("tool_call_"),
  );
  assert.equal(toolEvents.length, 10);
  assert.deepEqual(
    toolEvents.map((event) => event.wakeId),
    Array(10).fill(wakeId),
  );

  await native.unsubscribeEvents(brainEvents);

  console.log(
    JSON.stringify(
      {
        wakeId,
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        completionPackets: await native.countRows("completion_packets"),
        toolCallHistory: await native.countRows("tool_call_history"),
        toolEvents: toolEvents.map((event) => event.event.type),
        browserSessions: manager.diagnostics().sessions.length,
        cdpCalls: cdp.calls.map((call) => call.method),
      },
      null,
      2,
    ),
  );

  function bodyState(): BodyState {
    return {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId,
        profileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: selection.toolProfile,
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00Z",
        lastActiveAt: "2026-06-20T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    };
  }
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  await manager.closeAllForAgent(
    "production-browser-tools-agent" as AgentId,
    "manual",
  );
  rmSync(engineDataDir, { force: true, recursive: true });
}
