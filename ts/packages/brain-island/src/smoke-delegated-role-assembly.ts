import assert from "node:assert/strict";
import type {
  AgentId,
  ProfileId,
  SessionId,
  TaskId,
} from "@rusty-crew/contracts";
import {
  buildDelegatedRoleAssembly,
  normalizeDelegatedRole,
} from "./delegated-role-assembly.js";

const baseContext = {
  sessionId: "child-session" as SessionId,
  agentId: "agent:child-session" as AgentId,
  parentSessionId: "parent-session" as SessionId,
  parentAgentId: "prime-agent" as AgentId,
  sourceWakeId: "parent-wake-1",
  sourceActionIndex: 0,
  taskId: "2845" as TaskId,
  prompt: "Implement the bounded slice.",
  expectedOutput: "completion packet with implementation summary",
  correlationId: "delegation-correlation",
  resourceLimits: {
    workdir: "/home/dev/rusty-crew",
    maxDurationMs: 30_000,
    maxDelegationDepth: 0,
  },
  taskContext: "The parent has already read the relevant architecture docs.",
  acceptanceCriteria: [
    "Prompt includes parent lineage.",
    "Prompt includes resource limits.",
  ],
};

const coder = buildDelegatedRoleAssembly({
  role: "coder",
  profile: {
    profileId: "coder-profile" as ProfileId,
    displayName: "Coder",
    systemPrompt: "Profile-specific coder posture.",
    toolNames: ["read_file", "patch", "terminal"],
  },
  context: baseContext,
});

assert.match(coder.instructions ?? "", /bounded delegated coder/);
assert.match(coder.instructions ?? "", /parentSessionId: parent-session/);
assert.match(coder.instructions ?? "", /maxDelegationDepth: 0/);
assert.match(
  coder.instructions ?? "",
  /completion packet with implementation summary/,
);
assert.equal(
  coder.initialMessages?.[0]?.correlationId,
  "delegation-correlation",
);

const reviewer = buildDelegatedRoleAssembly({
  role: "reviewer",
  profile: {
    profileId: "reviewer-profile" as ProfileId,
    displayName: "Reviewer",
    toolNames: ["read_file", "git_diff"],
  },
  context: {
    ...baseContext,
    prompt: "Review the bounded slice.",
    expectedOutput: "review findings packet",
  },
});

assert.match(reviewer.instructions ?? "", /bounded delegated reviewer/);
assert.match(reviewer.instructions ?? "", /concrete findings/);

const auditor = buildDelegatedRoleAssembly({
  role: "packet-auditor",
  profile: {
    profileId: "packet-auditor-profile" as ProfileId,
    displayName: "Packet Auditor",
    toolNames: ["den_get_latest_worker_completion"],
  },
  context: {
    ...baseContext,
    prompt: "Audit the child completion packet.",
    expectedOutput: "packet audit report",
  },
});

assert.equal(normalizeDelegatedRole("packet-auditor"), "packet_auditor");
assert.match(auditor.instructions ?? "", /bounded packet auditor/);
assert.match(
  auditor.instructions ?? "",
  /completed, failed, blocked, and exhausted/,
);

console.log(
  JSON.stringify(
    {
      roles: ["coder", "reviewer", "packet_auditor"],
      coderInitialMessages: coder.initialMessages?.length ?? 0,
    },
    null,
    2,
  ),
);
