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
  createPiAgentBrain,
  createWebToolResolver,
  defaultBodyDeltaPolicy,
  registerBrainImplementationRuntime,
  selectToolProfile,
} from "./index.js";
import type {
  ResolvedAddress,
  ResolveHostAddresses,
  WebSearchProvider,
} from "./index.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-production-web-tools-engine-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

const providerCalls: Array<{ query: string; maxResults: number }> = [];
const provider: WebSearchProvider = {
  name: "fake-production-web",
  async search(query, maxResults) {
    providerCalls.push({ query, maxResults });
    return [
      {
        title: "Rusty Crew Architecture",
        url: "https://public.test/rusty-crew",
        snippet: "Unified runtime notes",
      },
    ];
  },
};

const resolvedHosts: Record<string, readonly ResolvedAddress[]> = {
  "public.test": [{ address: "93.184.216.34", family: 4 }],
};
const resolveHostAddresses: ResolveHostAddresses = async (hostname) =>
  resolvedHosts[hostname] ?? [];
const fetchCalls: string[] = [];
const fetchImpl: typeof fetch = async (input) => {
  const url = String(input);
  fetchCalls.push(url);
  if (url === "https://public.test/rusty-crew") {
    return new Response(
      "<html><title>Rusty Crew</title><body><main>Production wake proof.</main></body></html>",
      {
        status: 200,
        headers: { "content-type": "text/html; charset=utf-8" },
      },
    );
  }
  return new Response("missing", { status: 404 });
};

class WebToolCallingFakeAgent {
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
    await this.callTool("web_search", {
      query: "rusty crew architecture",
      max_results: 1,
    });
    await this.callTool("web_extract", {
      urls: ["https://public.test/rusty-crew"],
    });
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
  const sessionId = "production-web-tools-session" as SessionId;
  const agentId = "production-web-tools-agent" as AgentId;
  const profileId = "production-web-tools-profile" as ProfileId;
  const wakeId = "production-web-tools-wake";
  const selection = selectToolProfile({
    profileId,
    policy: {
      requestedToolsets: ["web_research"],
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
      implementationId: "production-web-tools" as BrainImplementationId,
      profileId,
      toolProfile: selection.toolProfile,
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createPiAgentBrain({
      createAgent: (options) => new WebToolCallingFakeAgent(options, outputs),
      resolveTools: createWebToolResolver({
        provider,
        fetchImpl,
        resolveHostAddresses,
      }),
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "production web tool wake completed",
          },
        },
      ],
    }),
  );

  const request = await native.buildBrainWakeRequest({
    brain,
    sessionId,
    bodyStateJson: encoder.encode(JSON.stringify(bodyState())),
    systemPrompt: "Use the selected production web tools.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({
        instructions: "Search for the runtime notes and extract the page.",
      }),
    ),
    wakeId,
  });

  const accepted = await native.wakeBrain(request);
  assert.deepEqual(accepted, { wakeId, accepted: true });
  assert.match(outputs.web_search, /Rusty Crew Architecture/);
  assert.match(outputs.web_extract, /Production wake proof/);
  assert.deepEqual(providerCalls, [
    { query: "rusty crew architecture", maxResults: 1 },
  ]);
  assert.deepEqual(fetchCalls, ["https://public.test/rusty-crew"]);
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("tool_call_history"), 4);

  const observedEvents = await native.drainSubscriptionEvents(brainEvents, 10);
  const toolEvents = observedEvents.filter(
    (event): event is Extract<CoreEvent, { type: "brain_event_observed" }> =>
      event.type === "brain_event_observed" &&
      event.event.type.startsWith("tool_call_"),
  );
  assert.deepEqual(
    toolEvents.map((event) => [event.wakeId, event.event.type]),
    [
      [wakeId, "tool_call_started"],
      [wakeId, "tool_call_finished"],
      [wakeId, "tool_call_started"],
      [wakeId, "tool_call_finished"],
    ],
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
        provider: provider.name,
        fetchCalls,
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
  rmSync(engineDataDir, { force: true, recursive: true });
}
