import assert from "node:assert/strict";
import test from "node:test";

import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { Type } from "typebox";

import type { BrainTool } from "../src/brain-tool.js";
import {
  BufferedBrainWakeError,
  runBufferedBrainHost,
} from "../src/buffered-brain-host.js";
import { buildProfileRoleAssembly } from "../src/profile-role-assembly.js";
import type { LoadedProfileContext } from "../src/profile-loading.js";
import {
  effectiveToolSelectionForResourceLimits,
  selectToolProfile,
} from "../src/tool-profile-selection.js";

test("zero delegation depth removes delegation tools from the native provider request", async () => {
  let modelVisibleTools: string[] = [];
  const bridge = {
    startBrainRun: async (
      input: Parameters<NativeBridgeModule["startBrainRun"]>[0],
    ) => {
      modelVisibleTools = (input.providerInput.tools ?? []).map(
        (tool) => tool.name,
      );
      return { moduleId: input.moduleId, wakeId: input.providerInput.wakeId };
    },
    drainBrainRun: async () => ({
      items: [],
      toolRequests: [],
      terminal: true,
    }),
  } as unknown as NativeBridgeModule;
  const toolProfile = {
    tools: [
      { name: "read_file", description: "Read files" },
      { name: "patch", description: "Apply patches" },
      { name: "scout_codebase", description: "Delegate scouting" },
      { name: "fan_out_subagents", description: "Delegate fan-out" },
    ],
  };

  await runBufferedBrainHost({
    bridge,
    moduleLabel: "OpenAI Responses",
    run: {
      moduleId: "openai-responses",
      providerInput: {
        wakeId: "zero-delegation-wake",
        sessionId: "zero-delegation-session" as SessionId,
        bodyState: wake(toolProfile).state,
        config: { model: "test-model" },
        client: { mode: "fake" },
      },
    },
    wake: wake(toolProfile),
    toolProfile,
    toolResolver: () => [
      fakeTool("read_file"),
      fakeTool("patch"),
      fakeTool("scout_codebase"),
      fakeTool("fan_out_subagents"),
    ],
  });

  assert.deepEqual(modelVisibleTools, ["read_file", "patch"]);
});

test("effective tool context reports delegation unavailable and keeps local editing", () => {
  const selection = effectiveToolSelectionForResourceLimits(
    selectToolProfile({
      profileId: "zero-delegation-profile" as ProfileId,
      policy: {
        requestedToolsets: [
          "local_code_read",
          "local_code_write",
          "delegation_basic",
        ],
      },
    }),
    { maxDelegationDepth: 0 },
  );
  const context = {
    profile: {
      profileId: "zero-delegation-profile" as ProfileId,
      modelConfig: { provider: "test", modelName: "test-model" },
    },
    skills: [],
    toolSelection: selection,
  } as LoadedProfileContext;
  const role = buildProfileRoleAssembly(context);

  assert.match(role.roleAssembly.instructions ?? "", /- read_file:/);
  assert.match(role.roleAssembly.instructions ?? "", /- patch:/);
  assert.match(
    role.roleAssembly.instructions ?? "",
    /scout_codebase: resource_denied \(delegation_depth_exhausted:/,
  );
  const selectedSection = (role.roleAssembly.instructions ?? "").split(
    "Unavailable tools:",
  )[0]!;
  assert.doesNotMatch(selectedSection, /- scout_codebase:/);
});

test("buffered native failures preserve terminal reason and transport metrics", async () => {
  const transportMetrics = {
    effectiveTransport: "http-sse",
    selectedStrategyId: "replay",
    effectiveStrategyId: "replay",
    fallbackReason: null,
    providerRequestCount: 3,
    continuationRoundCount: 2,
    providerRequestPayloadBytes: 123,
    providerEventCounts: {},
    firstTextDeltaLatencyMs: null,
    totalTurnDurationMs: 456,
    terminalFailureReasonCode: "provider_response_failed",
    terminalFailureSource: "provider_response",
  };
  const bridge = {
    startBrainRun: async () => ({
      moduleId: "openai-responses" as const,
      wakeId: "typed-failure-wake",
    }),
    drainBrainRun: async () => ({
      moduleId: "openai-responses" as const,
      wakeId: "typed-failure-wake",
      items: [
        {
          type: "wake_failed" as const,
          failure: {
            wakeId: "typed-failure-wake",
            sessionId: "zero-delegation-session" as SessionId,
            kind: "brain_unavailable" as const,
            reasonCode: "provider_response_failed",
            message: "provider rejected the response",
          },
        },
      ],
      toolRequests: [],
      terminal: true,
      terminalReasonCode: "provider_response_failed",
      transportMetrics,
      error: "provider rejected the response",
    }),
  } as unknown as NativeBridgeModule;

  await assert.rejects(
    runBufferedBrainHost({
      bridge,
      moduleLabel: "OpenAI Responses",
      run: {
        moduleId: "openai-responses",
        providerInput: {
          wakeId: "typed-failure-wake",
          sessionId: "zero-delegation-session" as SessionId,
          bodyState: wake({ tools: [] }).state,
          config: { model: "test-model" },
          client: { mode: "fake" },
        },
      },
      wake: { ...wake({ tools: [] }), wakeId: "typed-failure-wake" },
      toolProfile: { tools: [] },
    }),
    (error: unknown) => {
      assert.ok(error instanceof BufferedBrainWakeError);
      assert.equal(error.reasonCode, "provider_response_failed");
      assert.equal(
        error.transportMetrics?.terminalFailureSource,
        "provider_response",
      );
      assert.equal(error.brainStreamItemCounts?.wake_failed, 1);
      return true;
    },
  );
});

function wake(toolProfile: {
  tools: Array<{ name: string; description: string }>;
}) {
  const sessionId = "zero-delegation-session" as SessionId;
  return {
    wakeId: "zero-delegation-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "instructions" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "zero-delegation-agent" as AgentId,
        profileId: "zero-delegation-profile" as ProfileId,
        kind: "full" as const,
        resourceLimits: { maxDelegationDepth: 0 },
        toolProfile,
        status: "idle" as const,
        brainTurnCount: 0,
        createdAt: "2026-07-18T00:00:00Z",
        lastActiveAt: "2026-07-18T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake" as const,
        queueOwner: "body" as const,
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  };
}

function fakeTool(name: string): BrainTool {
  return {
    name,
    label: name,
    description: `${name} description`,
    parameters: Type.Object({}),
    execute: async () => ({
      content: [{ type: "text", text: `${name} result` }],
      details: {},
    }),
  };
}
