import assert from "node:assert/strict";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  buildProfileRoleAssembly,
  createPiAgentBrain,
  loadProfileContext,
  renderDenMemoryContext,
  renderDenseProfileMemoryContext,
  renderPlanningContext,
  renderSessionTodoContext,
} from "./index.js";
import type { PiAgentLike } from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-role-assembly-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  writeFileSync(
    join(profilesDir, "reviewer.json"),
    JSON.stringify(
      {
        profileId: "reviewer",
        displayName: "Reviewer",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        runtime: {
          maxTurns: 2,
          defaultResourceLimits: {
            workdir: "/home/dev/rusty-crew",
            maxDurationMs: 10_000,
            maxDelegationDepth: 0,
          },
        },
        toolPolicy: {
          requestedToolsets: ["review_readonly", "local_code_write"],
          deniedTools: ["terminal"],
        },
        prompt: {
          system: "Profile system prompt wins by default.",
          instructions: ["Inspect before judging.", "Prefer concise findings."],
          soulMarkdown: "You are a careful reviewer.",
          memoryMarkdown: "Reviewers prefer concrete evidence.",
        },
        skills: ["review-rubric"],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(skillsDir, "review-rubric.md"),
    `---
title: Review Rubric
summary: Check behavior, tests, and maintainability.
tags:
  - review
---

Look for concrete regressions and cite evidence.
`,
  );

  const context = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "reviewer" as ProfileId,
    session: {
      readOnly: true,
    },
  });
  const assembled = buildProfileRoleAssembly(context, {
    additionalInstructions: ["Do not invent findings."],
    denMemoryContext: renderDenMemoryContext({
      mode: "metadata",
      projectId: "rusty-crew",
      profileId: "reviewer",
      guidance: "Recall only durable facts relevant to the current work.",
    }),
    denseProfileMemoryContext: renderDenseProfileMemoryContext([
      {
        targetType: "profile",
        key: "review-style",
        content: "Prefer high-signal evidence over broad commentary.",
        revision: 2,
      },
    ]),
    planningContext: renderPlanningContext({
      todoContext: renderSessionTodoContext({
        sessionId: "profile-role-session",
        items: [
          {
            id: "review-pass",
            title: "Check role assembly ordering",
            status: "pending",
          },
        ],
      }),
      sessionSearchGuidance:
        "Use session_search for Rust-owned session and message history.",
      counterGuidance:
        "Use runtime counters only as derived health/debug projections.",
    }),
  });
  assert.equal(
    assembled.systemPrompt,
    "Profile system prompt wins by default.",
  );

  const instructions = assembled.roleAssembly.instructions ?? "";
  assertOrder(instructions, [
    "# Profile",
    "# Profile Soul",
    "# Profile Memory",
    "# Profile Instructions",
    "# Den Memory",
    "# Dense Profile Memory",
    "# Selected Skills",
    "# Tool Inventory",
    "# Planning Context",
    "# Runtime",
    "# Additional Instructions",
  ]);
  assert.match(instructions, /Review Rubric/);
  assert.match(instructions, /careful reviewer/);
  assert.match(instructions, /concrete evidence/);
  assert.match(instructions, /Den-owned memory/);
  assert.match(instructions, /review-style/);
  assert.match(instructions, /Session Search/);
  assert.match(instructions, /Session Todo/);
  assert.match(instructions, /read_file/);
  assert.match(instructions, /terminal: profile_denied/);
  assert.match(instructions, /patch: resource_denied/);

  const overridden = buildProfileRoleAssembly(context, {
    systemPromptOverride: "Explicit system override.",
    includeSkillBodies: false,
  });
  assert.equal(overridden.systemPrompt, "Explicit system override.");
  assert.doesNotMatch(
    overridden.roleAssembly.instructions ?? "",
    /concrete regressions/,
  );

  let capturedSystemPrompt = "";
  const brain = createPiAgentBrain({
    createAgent(options: PiAgentOptions): PiAgentLike {
      capturedSystemPrompt = options.initialState?.systemPrompt ?? "";
      return createStubPiAgent();
    },
  });
  await brain.wake({
    wakeId: "profile-role-wake",
    sessionId: "profile-role-session" as SessionId,
    systemPrompt: assembled.systemPrompt,
    roleAssembly: assembled.roleAssembly,
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId: "profile-role-session" as SessionId,
        agentId: "profile-role-agent" as AgentId,
        profileId: "reviewer" as ProfileId,
        kind: "full",
        resourceLimits: {},
        toolProfile: context.toolSelection.toolProfile,
        status: "active",
        brainTurnCount: 0,
        createdAt: "2026-06-19T00:00:00Z",
        lastActiveAt: "2026-06-19T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake",
        queueOwner: "body",
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  });

  assert.match(capturedSystemPrompt, /Profile system prompt wins by default/);
  assert.match(capturedSystemPrompt, /Review Rubric/);

  console.log(
    JSON.stringify(
      {
        systemPrompt: assembled.systemPrompt,
        sections: [
          "Profile",
          "Profile Instructions",
          "Den Memory",
          "Dense Profile Memory",
          "Selected Skills",
          "Tool Inventory",
          "Planning Context",
          "Runtime",
          "Additional Instructions",
        ],
        capturedPromptIncludesSkill: /Review Rubric/.test(capturedSystemPrompt),
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

function assertOrder(text: string, markers: readonly string[]): void {
  let previous = -1;
  for (const marker of markers) {
    const index = text.indexOf(marker);
    assert.ok(index > previous, `${marker} should appear after prior marker`);
    previous = index;
  }
}

function createStubPiAgent(): PiAgentLike {
  let listener: Parameters<PiAgentLike["subscribe"]>[0] | undefined;

  return {
    subscribe(callback) {
      listener = callback;
      return () => {
        listener = undefined;
      };
    },
    async prompt(_input: PiAgentMessage | PiAgentMessage[] | string) {
      await listener?.(
        { type: "agent_start" } as PiAgentEvent,
        new AbortController().signal,
      );
      await listener?.(
        { type: "agent_end" } as PiAgentEvent,
        new AbortController().signal,
      );
    },
    async waitForIdle() {},
    clearAllQueues() {},
  };
}
