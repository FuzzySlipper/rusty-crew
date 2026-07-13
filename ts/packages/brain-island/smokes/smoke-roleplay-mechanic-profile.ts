import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import {
  loadProfileContext,
  ProfileLoadError,
} from "../src/profile-loading.js";
import { getMechanicCapabilitiesTool } from "../src/roleplay-mechanic-tools.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-roleplay-mechanic-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  const native = await loadNativeBridge();
  writeProfile("mechanic", {
    profileId: "mechanic",
    displayName: "Maren",
    modelConfig: {
      provider: "test",
      modelName: "mechanic-model",
    },
    localToolProfileId: "roleplay_mechanic",
    toolPolicy: {
      requestedToolsets: ["roleplay_mechanic"],
      requestedTools: [],
    },
    roleplayMechanic: { autoMonitor: false },
  });
  writeProfile("narrator", {
    profileId: "narrator",
    modelConfig: {
      provider: "test",
      modelName: "narrator-model",
    },
    localToolProfileId: "roleplay_lore",
    toolPolicy: {
      requestedToolsets: [
        "roleplay_lore_read",
        "roleplay_lore_write",
        "roleplay_lore_manage",
        "roleplay_scene_state",
      ],
      requestedTools: [],
    },
    roleplayNarrator: {},
  });

  const mechanic = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "mechanic" as ProfileId,
  });
  assert.deepEqual(mechanic.profile.roleplayMechanic, {
    autoMonitor: false,
  });
  assert.deepEqual(
    mechanic.toolSelection.toolProfile.tools.map((tool) => tool.name),
    [
      "search_lore",
      "list_lore_layers",
      "get_mechanic_capabilities",
      "inspect_roleplay_transcript",
      "inspect_roleplay_scene",
      "inspect_lore_retrieval",
      "inspect_roleplay_proposals",
      "propose_roleplay_change",
    ],
  );

  const narrator = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "narrator" as ProfileId,
  });
  assert.equal(
    narrator.toolSelection.toolProfile.tools.some(
      (tool) =>
        tool.name.startsWith("inspect_") ||
        tool.name === "get_mechanic_capabilities" ||
        tool.name === "propose_roleplay_change",
    ),
    false,
  );

  const plan = await native.planRoleplayMechanicProfile({
    name: mechanic.profile.displayName,
    autoMonitor: false,
  });
  assert.equal(plan.localToolProfileId, "roleplay_mechanic");
  assert.match(plan.systemPrompt, /environmental diagnostician/i);
  assert.match(plan.systemPrompt, /do not narrate/i);
  assert.match(plan.systemPrompt, /proposal for user review/i);

  const result = await getMechanicCapabilitiesTool({
    bridge: native,
    profile: mechanic.profile,
  }).execute("capabilities", {});
  assert.equal(result.details.ok, true);
  assert.equal(result.details.action, "read");
  assert.deepEqual(result.details.result, {
    config: plan.config,
    localToolProfileId: "roleplay_mechanic",
    mutationPolicy: "proposal_only",
    directStateWrites: false,
  });

  writeProfile("invalid-monitor", {
    profileId: "invalid-monitor",
    modelConfig: { provider: "test", modelName: "mechanic-model" },
    roleplayMechanic: { autoMonitor: true },
  });
  await assert.rejects(
    () =>
      loadProfileContext({
        profilesDir,
        skillsDir,
        profileId: "invalid-monitor" as ProfileId,
      }),
    (error) =>
      error instanceof ProfileLoadError &&
      /autoMonitor is not available and must be false/.test(error.message),
  );

  console.log(
    JSON.stringify(
      {
        profileId: mechanic.profile.profileId,
        selectedTools: mechanic.toolSelection.toolProfile.tools.map(
          (tool) => tool.name,
        ),
        narratorHasMechanicTool: false,
        autoMonitor: plan.config.autoMonitor,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

function writeProfile(profileId: string, value: Record<string, unknown>): void {
  writeFileSync(
    join(profilesDir, `${profileId}.json`),
    JSON.stringify(value, null, 2),
  );
}
