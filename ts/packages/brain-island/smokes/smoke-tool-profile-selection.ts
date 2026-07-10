import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentId,
  AdapterId,
  BrainImplementationId,
  BrainWakeRequest,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  buildBrainRegistrationFromToolProfile,
  createToolCatalogChangedPayload,
  registerBrainImplementationRuntime,
  selectToolProfile,
} from "../src/index.js";
import { createLocalBrain } from "./support/local-brain-test-support.js";
import type { BrainRoleAssembly } from "../src/index.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const engineDataDir = mkdtempSync(
  join(tmpdir(), "rusty-crew-tool-profile-selection-"),
);
const native = await loadNativeBridge();
const engine = await native.initializeEngine({
  engineDataDir,
  clock: { fixed: "2026-06-19T00:00:00Z" },
  defaultTurnBudget: 3,
  defaultIdleTimeoutMs: 1_000,
});

try {
  const parentSessionId = "tool-profile-parent" as SessionId;
  const parentAgentId = "tool-profile-parent-agent" as AgentId;
  const parentProfileId = "tool-profile-parent-profile" as ProfileId;
  const delegatedProfileId = "tool-profile-delegated-profile" as ProfileId;
  const delegatedSessionId =
    "tool-profile-parent:delegated:tool-profile-wake:0" as SessionId;

  const readonlySelection = selectToolProfile({
    profileId: delegatedProfileId,
    policy: {
      requestedToolsets: ["local_code_read", "local_code_write"],
    },
    session: {
      readOnly: true,
    },
  });
  assert.deepEqual(
    readonlySelection.toolProfile.tools.map((tool) => tool.name),
    ["read_file", "search_files", "git_status", "git_diff"],
  );
  assert.equal(
    readonlySelection.inventory.items.find((item) => item.name === "terminal")
      ?.status,
    "resource_denied",
  );

  const unavailableMemoryPlan = await native.planToolAvailability({
    selectedTools: [
      "memory_recall",
      "memory_search",
      "memory_store",
      "mcp_den_documents_read",
    ],
    denMemory: {
      configured: true,
      clientAvailable: false,
      mode: "candidate",
      lastError: "memory endpoint timed out",
    },
  });
  assert.deepEqual(unavailableMemoryPlan.selectedTools, [
    "mcp_den_documents_read",
  ]);
  assert.deepEqual(
    unavailableMemoryPlan.omittedTools.map((omission) => omission.toolName),
    ["memory_recall", "memory_search", "memory_store"],
  );
  assert.equal(
    unavailableMemoryPlan.omittedTools[0]?.reasonCode,
    "memory_external_dependency_unavailable",
  );

  const metadataMemoryPlan = await native.planToolAvailability({
    selectedTools: [
      "memory_recall",
      "memory_search",
      "memory_store",
      "memory_propose",
    ],
    denMemory: {
      configured: true,
      clientAvailable: true,
      mode: "metadata",
    },
  });
  assert.deepEqual(metadataMemoryPlan.selectedTools, [
    "memory_recall",
    "memory_search",
  ]);
  assert.deepEqual(
    metadataMemoryPlan.omittedTools.map((omission) => omission.toolName),
    ["memory_store", "memory_propose"],
  );

  const registration = buildBrainRegistrationFromToolProfile({
    implementationId: "tool-profile-selection" as BrainImplementationId,
    profileId: delegatedProfileId,
    modelConfig: {
      provider: "local",
      modelName: "deterministic",
    },
    policy: {
      requestedToolsets: ["local_code_read"],
      requestedTools: ["terminal"],
      deniedTools: ["terminal"],
    },
  });
  assert.deepEqual(
    registration.toolProfile.tools.map((tool) => tool.name),
    ["read_file", "search_files", "git_status", "git_diff"],
  );

  const brain = await registerBrainImplementationRuntime(
    native,
    registration,
    createLocalBrain(),
  );
  await native.createSession({
    sessionId: parentSessionId,
    agentId: parentAgentId,
    profileId: parentProfileId,
    kind: "full",
  });
  const catalogReceipt = await native.injectExternalEvent({
    adapterId: "tool-registry" as AdapterId,
    source: "tool-profile-selection-smoke",
    payload: createToolCatalogChangedPayload("default-local-tools"),
  });
  assert.equal(catalogReceipt.accepted, true);

  const receipt = await native.diagnosticSubmitBrainActionsJson(
    "tool-profile-wake",
    parentSessionId,
    [
      {
        type: "request_delegation",
        profileId: delegatedProfileId,
        prompt: "Verify selected tool profile arrives at the delegated brain.",
      },
    ],
  );
  assert.equal(receipt.acceptedActions, 1);

  const request = await native.buildBrainWakeRequestForSession({
    brain,
    sessionId: delegatedSessionId,
    systemPrompt: "Tool profile selection smoke.",
    roleAssemblyJson: encoder.encode(
      JSON.stringify({
        instructions: "Inspect the selected tool profile.",
      } satisfies BrainRoleAssembly),
    ),
    wakeId: "tool-profile-child-wake",
  });
  const bodyState = await readBodyState(request);
  assert.deepEqual(
    bodyState.session.tool_profile.tools.map((tool) => tool.name),
    ["read_file", "search_files", "git_status", "git_diff"],
  );

  console.log(
    JSON.stringify(
      {
        selectedTools: registration.toolProfile.tools.map((tool) => tool.name),
        delegatedSessionTools: bodyState.session.tool_profile.tools.map(
          (tool) => tool.name,
        ),
        catalogChangedAccepted: catalogReceipt.accepted,
      },
      null,
      2,
    ),
  );
} finally {
  await native.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
  rmSync(engineDataDir, { force: true, recursive: true });
}

async function readBodyState(request: BrainWakeRequest): Promise<{
  session: { tool_profile: { tools: Array<{ name: string }> } };
}> {
  const view = await native.getBuffer(request.bodyState);
  try {
    return JSON.parse(decoder.decode(view.bytes)) as {
      session: { tool_profile: { tools: Array<{ name: string }> } };
    };
  } finally {
    await native.releaseBuffer(request.bodyState);
    await native.releaseBuffer(request.systemPrompt);
    await native.releaseBuffer(request.roleAssembly);
  }
}
