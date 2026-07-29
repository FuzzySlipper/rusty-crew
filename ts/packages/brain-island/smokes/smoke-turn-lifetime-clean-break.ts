import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import type { ProfileId } from "@rusty-crew/contracts";

import { ADMIN_CONTROL_CAPABILITIES } from "../src/api-command-registry.js";
import { loadProfileConfig } from "../src/profile-loading.js";
import { loadRustyCrewServiceConfig } from "../src/service-config.js";
import { loadRustyCrewRuntimeConfig } from "../src/service-runtime-config.js";

const root = await mkdtemp(join(tmpdir(), "rusty-crew-turn-lifetime-"));
const profilesDir = join(root, "profiles");
const service = loadRustyCrewServiceConfig({
  RUSTY_CREW_DATA_DIR: root,
  RUSTY_CREW_ADMIN_AUTH_MODE: "none",
});

try {
  await mkdir(service.paths.configDir, { recursive: true });
  await mkdir(profilesDir, { recursive: true });

  await writeFile(
    service.paths.serviceConfigFile,
    JSON.stringify({ wakeTimeout: { mode: "disabled" } }),
  );
  await assert.rejects(
    () => loadRustyCrewRuntimeConfig(service),
    /wakeTimeout is retired/,
  );

  await writeFile(
    service.paths.serviceConfigFile,
    JSON.stringify({
      sessions: [
        {
          sessionId: "retired-timeout-session",
          agentId: "retired-timeout-profile",
          profileId: "retired-timeout-profile",
          turnTimeoutMs: 60_000,
        },
      ],
    }),
  );
  await assert.rejects(
    () => loadRustyCrewRuntimeConfig(service),
    /sessions\[0\]\.turnTimeoutMs is retired/,
  );

  await writeFile(
    join(profilesDir, "retired-timeout-profile.json"),
    JSON.stringify({
      profileId: "retired-timeout-profile",
      modelConfig: { provider: "local", modelName: "deterministic" },
      runtime: { maxTurnDurationMs: 60_000 },
    }),
  );
  await assert.rejects(
    () =>
      loadProfileConfig(profilesDir, "retired-timeout-profile" as ProfileId),
    /runtime\.maxTurnDurationMs is retired/,
  );

  assert.equal(
    ADMIN_CONTROL_CAPABILITIES.some(
      (capability) =>
        capability.command_name.includes("wake_timeout") ||
        capability.path_template.includes("wake-timeout"),
    ),
    false,
  );
} finally {
  await rm(root, { recursive: true, force: true });
}

console.log("whole-turn lifetime clean-break smoke passed");
