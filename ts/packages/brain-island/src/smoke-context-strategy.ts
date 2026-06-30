import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ProfileId } from "@rusty-crew/contracts";
import {
  buildProfileRoleAssembly,
  defaultContextStrategyPolicy,
  loadProfileContext,
  prepareContextStrategyRoleAssembly,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-context-strategy-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  writeFileSync(
    join(profilesDir, "context-profile.json"),
    JSON.stringify(
      {
        profileId: "context-profile",
        displayName: "Context Profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        prompt: {
          system: "Context strategy smoke.",
          instructions: ["Keep context strategy behavior explicit."],
        },
      },
      null,
      2,
    ),
  );

  const profileContext = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "context-profile" as ProfileId,
  });
  const defaultPreparation = prepareContextStrategyRoleAssembly(
    defaultContextStrategyPolicy(),
  );
  const defaultAssembly = buildProfileRoleAssembly(profileContext, {
    additionalInstructions: defaultPreparation.additionalInstructions,
  });
  assert.equal(defaultPreparation.strategyId, "recent_window");
  assert.equal(defaultPreparation.additionalInstructions.length, 0);
  assert.doesNotMatch(
    defaultAssembly.roleAssembly.instructions ?? "",
    /Context strategy:/,
  );

  const augmentedPreparation = prepareContextStrategyRoleAssembly({
    ...defaultContextStrategyPolicy(),
    strategyId: "session_memory_augmented",
  });
  const augmentedAssembly = buildProfileRoleAssembly(profileContext, {
    additionalInstructions: augmentedPreparation.additionalInstructions,
  });
  assert.equal(augmentedPreparation.strategyId, "session_memory_augmented");
  assert.match(
    augmentedAssembly.roleAssembly.instructions ?? "",
    /Context strategy: session_memory_augmented/,
  );
  assert.match(
    augmentedAssembly.roleAssembly.instructions ?? "",
    /Rust-selected session memory context/,
  );

  console.log(
    JSON.stringify(
      {
        defaultStrategy: defaultPreparation.strategyId,
        defaultInstructions: defaultPreparation.additionalInstructions.length,
        alternateStrategy: augmentedPreparation.strategyId,
        alternateInstructions:
          augmentedPreparation.additionalInstructions.length,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { force: true, recursive: true });
}
