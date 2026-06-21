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
  writeRuntimeConfig(root);
  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
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

    assert.deepEqual(selectedToolNames, ["git_status"]);
    assert.match(outputs.git_status ?? "", /git status --short/);
    assert.equal(await native.diagnosticCountRows("tool_call_history"), 2);
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
      ["tool_call_started", "tool_call_finished"],
    );

    console.log(
      JSON.stringify(
        {
          selectedToolNames,
          toolHistoryRows:
            await native.diagnosticCountRows("tool_call_history"),
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
          requestedTools: ["git_status"],
        },
      },
      null,
      2,
    ),
  );
}
