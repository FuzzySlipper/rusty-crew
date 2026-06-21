import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentTool,
} from "@earendil-works/pi-agent-core";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import { loadRustyCrewServiceConfig } from "./service-config.js";
import {
  applyRustyCrewRuntimeConfig,
  loadRustyCrewRuntimeConfig,
} from "./service-runtime-config.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const root = mkdtempSync(
  join(tmpdir(), "rusty-crew-service-den-router-tools-"),
);
const native = await loadNativeBridge();
const originalFetch = globalThis.fetch;
const memoryRequests: Array<{
  url: string;
  authorization?: string;
  body: Record<string, unknown>;
}> = [];
const requestedToolNames = [
  "browser_snapshot",
  "channel_readback",
  "counter_reset",
  "curator_execute",
  "den_memory_recall",
  "dense_profile_memory",
  "git_status",
  "session_search",
  "skills_list",
  "todo",
  "web_search",
].sort();

class ToolCallingFakeAgent {
  private listener?: (event: PiAgentEvent, signal: AbortSignal) => void;

  constructor(
    private readonly options: PiAgentOptions,
    private readonly outputs: Record<string, string>,
    private readonly selectedToolNames: string[],
  ) {
    this.selectedToolNames.push(
      ...(this.options.initialState?.tools ?? []).map((tool) => tool.name),
    );
  }

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
    await this.callTool("den_memory_recall", {
      prompt: "What memory guidance is relevant?",
    });
    await this.callTool("git_status", {});
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callTool(
    name: string,
    params: Record<string, unknown>,
  ): Promise<void> {
    const tool = this.options.initialState?.tools?.find(
      (candidate) => candidate.name === name,
    );
    assert.ok(tool, `${name} should be callable`);
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
  globalThis.fetch = (async (input, init) => {
    const url = new URL(String(input));
    const body =
      typeof init?.body === "string"
        ? (JSON.parse(init.body) as Record<string, unknown>)
        : {};
    memoryRequests.push({
      url: url.toString(),
      authorization: header(init?.headers, "authorization"),
      body,
    });
    assert.equal(url.pathname, "/memory/recall");
    return new Response(
      JSON.stringify({
        ok: true,
        data: {
          memories: [
            {
              id: "service-memory-1",
              summary: "Den memory is available through service config.",
            },
          ],
          total: 1,
        },
      }),
      { status: 200, headers: { "content-type": "application/json" } },
    );
  }) satisfies typeof fetch;

  writeRuntimeConfig(root);
  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    RUSTY_CREW_DEN_MEMORY_BASE_URL: "http://den-memory.local",
    RUSTY_CREW_DEN_MEMORY_TOKEN: "memory-token",
    RUSTY_CREW_DEN_MEMORY_RECALL_PATH: "/memory/recall",
  });
  const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
  const engine = await native.initializeEngine({
    engineDataDir: serviceConfig.paths.engineDataDir,
    clock: { fixed: "2026-06-21T12:20:00Z" },
    defaultTurnBudget: 8,
    defaultIdleTimeoutMs: 1_000,
  });
  const outputs: Record<string, string> = {};
  const selectedToolNames: string[] = [];
  try {
    const applyResult = await applyRustyCrewRuntimeConfig({
      serviceConfig,
      runtimeConfig,
      bridge: native,
      createDenRouterAgentFactory: async () => (options) =>
        new ToolCallingFakeAgent(options, outputs, selectedToolNames),
    });
    const brain = applyResult.brainHandlesByProfileId["field-profile"];
    assert.ok(brain, "field-profile brain should be registered");

    const subscription = await native.subscribeEvents({
      eventKinds: ["brain_event_observed"],
      sessionId: "field-session" as SessionId,
    });
    const request = await native.buildBrainWakeRequestForSession({
      brain,
      sessionId: "field-session" as SessionId,
      systemPrompt: "Use the configured tools.",
      roleAssemblyJson: encoder.encode(
        JSON.stringify({
          instructions: "Call git_status and summarize the result.",
        }),
      ),
      wakeId: "service-den-router-tools-wake",
    });
    await native.wakeBrain(request);
    const events = await native.drainSubscriptionEvents(subscription, 16);
    await native.unsubscribeEvents(subscription);

    assert.deepEqual([...selectedToolNames].sort(), requestedToolNames);
    assert.match(outputs.git_status ?? "", /git status --short/);
    assert.match(
      outputs.den_memory_recall ?? "",
      /Den memory is available through service config/,
    );
    assert.equal(memoryRequests.length, 1);
    assert.equal(memoryRequests[0]?.authorization, "Bearer memory-token");
    assert.equal(
      memoryRequests[0]?.body["prompt"],
      "What memory guidance is relevant?",
    );
    assert.equal(await native.diagnosticCountRows("tool_call_history"), 4);
    assert.deepEqual(
      events
        .filter((event) => event.type === "brain_event_observed")
        .map((event) =>
          event.event.type === "tool_call_started" ||
          event.event.type === "tool_call_finished"
            ? event.event.type
            : undefined,
        )
        .filter(Boolean),
      [
        "tool_call_started",
        "tool_call_finished",
        "tool_call_started",
        "tool_call_finished",
      ],
    );

    console.log(
      JSON.stringify(
        {
          selectedToolNames,
          toolHistoryRows:
            await native.diagnosticCountRows("tool_call_history"),
          memoryRequests: memoryRequests.length,
          gitStatusFirstLine: outputs.git_status?.split("\n")[0],
        },
        null,
        2,
      ),
    );
  } finally {
    await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  }
} finally {
  globalThis.fetch = originalFetch;
  rmSync(root, { force: true, recursive: true });
}

function writeRuntimeConfig(targetRoot: string): void {
  const configDir = join(targetRoot, "config");
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        brains: [{ profileId: "field-profile" }],
        sessions: [
          {
            sessionId: "field-session",
            agentId: "field-agent",
            profileId: "field-profile",
            kind: "full",
          },
        ],
        channelBindings: [],
        mcpBindings: [],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "field-profile.json"),
    JSON.stringify(
      {
        profileId: "field-profile",
        modelConfig: {
          provider: "den-router",
          modelName: "fake-router-model",
        },
        toolPolicy: {
          requestedTools: requestedToolNames,
        },
      },
      null,
      2,
    ),
  );
}

function header(
  headers: HeadersInit | undefined,
  name: string,
): string | undefined {
  if (!headers) return undefined;
  if (headers instanceof Headers) return headers.get(name) ?? undefined;
  if (Array.isArray(headers)) {
    return headers.find(([key]) => key.toLowerCase() === name)?.[1];
  }
  return headers[name];
}
