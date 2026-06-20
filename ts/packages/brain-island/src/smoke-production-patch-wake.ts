import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
  defaultBodyDeltaPolicy,
  registerBrainImplementationRuntime,
  resolveLocalCodeTools,
  selectToolProfile,
} from "./index.js";

const encoder = new TextEncoder();
const abortSignal = new AbortController().signal;
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-production-patch-engine-"),
);
const workdir = mkdtempSync(
  join(tmpdir(), "rusty-crew-production-patch-workdir-"),
);
writeFileSync(join(workdir, "target.txt"), "before patch\n", "utf8");

const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-20T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

class PatchCallingFakeAgent {
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
    await this.callPatch();
    await this.emit({ type: "agent_end", messages: [] } as PiAgentEvent);
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}

  private async callPatch() {
    const patch = this.options.initialState?.tools?.find(
      (tool) => tool.name === "patch",
    );
    assert.ok(patch);
    await this.emit({
      type: "tool_execution_start",
      toolName: "patch",
    } as PiAgentEvent);
    try {
      const result = await (patch as AgentTool).execute("patch-call", {
        path: "target.txt",
        old_string: "before patch",
        new_string: "after patch",
      });
      this.outputs.patch = result.content
        .flatMap((content) =>
          content.type === "text" && typeof content.text === "string"
            ? [content.text]
            : [],
        )
        .join("");
      await this.emit({
        type: "tool_execution_end",
        toolName: "patch",
        isError: false,
      } as PiAgentEvent);
    } catch (error) {
      await this.emit({
        type: "tool_execution_end",
        toolName: "patch",
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
  const sessionId = "production-patch-session" as SessionId;
  const agentId = "production-patch-agent" as AgentId;
  const profileId = "production-patch-profile" as ProfileId;
  const wakeId = "production-patch-wake";
  const selection = selectToolProfile({
    profileId,
    policy: {
      requestedTools: ["patch"],
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
      implementationId: "production-patch" as BrainImplementationId,
      profileId,
      toolProfile: selection.toolProfile,
      modelConfig: { provider: "local", modelName: "deterministic" },
    },
    createPiAgentBrain({
      createAgent: (options) => new PatchCallingFakeAgent(options, outputs),
      resolveTools: resolveLocalCodeTools,
      planActions: ({ wake }): BrainAction[] => [
        {
          type: "deliver_completion",
          packet: {
            sessionId: wake.sessionId,
            status: "completed",
            summary: "production patch wake completed",
          },
        },
      ],
    }),
  );

  const request = await native.buildBrainWakeRequest({
    brain,
    sessionId,
    bodyStateJson: encoder.encode(JSON.stringify(bodyState())),
    systemPrompt: "Use the selected production patch tool.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({
        instructions: "Patch target.txt from before patch to after patch.",
      }),
    ),
    wakeId,
  });

  const accepted = await native.wakeBrain(request);
  assert.deepEqual(accepted, { wakeId, accepted: true });
  assert.equal(
    readFileSync(join(workdir, "target.txt"), "utf8"),
    "after patch\n",
  );
  assert.match(outputs.patch, /--- target\.txt/);
  assert.match(outputs.patch, /\+after patch/);
  assert.equal(await native.countRows("completion_packets"), 1);
  assert.equal(await native.countRows("tool_call_history"), 2);

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
    ],
  );

  await native.unsubscribeEvents(brainEvents);

  console.log(
    JSON.stringify(
      {
        wakeId,
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        patchedText: readFileSync(join(workdir, "target.txt"), "utf8").trim(),
        completionPackets: await native.countRows("completion_packets"),
        toolCallHistory: await native.countRows("tool_call_history"),
        toolEvents: toolEvents.map((event) => event.event.type),
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
        resourceLimits: {
          workdir,
          maxDurationMs: 5_000,
        },
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
  rmSync(workdir, { force: true, recursive: true });
}
