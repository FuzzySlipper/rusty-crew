import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type { ProfileId } from "@rusty-crew/contracts";
import type {
  NativeCreateProfileRequest,
  NativeRuntimeConfigValidationInput,
} from "@rusty-crew/native-bridge";
import { loadProfileConfig } from "./profile-loading.js";
import {
  planCreateProfileWithRust,
  runtimeConfigValidationInput,
} from "./runtime-config-validation.js";
import { loadRustyCrewServiceConfig } from "./service-config.js";
import {
  loadRustyCrewRuntimeConfig,
  preflightRustyCrewRuntimeConfig,
} from "./service-runtime-config.js";

const fixtureRoot = fileURLToPath(
  new URL("../../../../fixtures/runtime-config-parity/", import.meta.url),
);
const tempRoot = await mkdtemp(join(tmpdir(), "rusty-crew-config-parity-"));

try {
  await mkdir(join(tempRoot, "config"), { recursive: true });
  await mkdir(join(tempRoot, "profiles"), { recursive: true });
  await mkdir(join(tempRoot, "skills"), { recursive: true });
  await writeFixture(
    "valid/service.json",
    join(tempRoot, "config", "service.json"),
    tempRoot,
  );
  await writeFixture(
    "valid/profiles/parity-runner.json",
    join(tempRoot, "profiles", "parity-runner.json"),
    tempRoot,
  );

  const bridge = await loadNativeBridge();
  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: tempRoot,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
  });
  const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
  const profile = await loadProfileConfig(
    join(tempRoot, "profiles"),
    "parity-runner" as ProfileId,
  );

  const actualInput = jsonRoundTrip(
    runtimeConfigValidationInput(runtimeConfig, [profile]),
  );
  const expectedInput =
    await readFixtureJson<NativeRuntimeConfigValidationInput>(
      "valid/validation-input.camel.json",
      tempRoot,
    );
  assert.deepEqual(
    actualInput,
    expectedInput,
    "TS runtime/profile loading drifted from the shared config validation fixture",
  );

  const validation = await bridge.validateRuntimeConfigDraft(expectedInput);
  assert.deepEqual(validation.diagnostics, []);

  const plan = await bridge.planRuntimeConfig(expectedInput);
  assert.deepEqual(plan.diagnostics, []);
  assert.deepEqual(
    jsonRoundTrip(plan.runtimeConfig),
    expectedInput.runtimeConfig,
  );
  assert.deepEqual(jsonRoundTrip(plan.derivedScheduledJobs), []);
  assert.deepEqual(jsonRoundTrip(plan.derivedMcpBindings), []);

  const createRequest = await readFixtureJson<NativeCreateProfileRequest>(
    "valid/create-profile-request.camel.json",
    tempRoot,
  );
  const createPlan = await planCreateProfileWithRust({
    bridge,
    runtimeConfig,
    profiles: [profile],
    request: createRequest,
  });
  assert.deepEqual(createPlan.diagnostics, []);
  assert.equal(createPlan.profileSeed?.profileId, "parity-created");
  assert.equal(
    createPlan.runtimeBrain?.implementationId,
    "parity-created-brain",
  );
  assert.equal(createPlan.runtimeSession?.sessionId, "parity-created-session");
  assert.equal(createPlan.profileMcpConfig?.toolProfile, "planner");

  await writeFixture(
    "invalid/service.json",
    join(tempRoot, "config", "service.json"),
    tempRoot,
  );
  const invalidReport = await preflightRustyCrewRuntimeConfig({
    serviceConfig,
    bridge,
  });
  assert.equal(invalidReport.ok, false);
  assert.deepEqual(
    new Set(invalidReport.diagnostics.map((diagnostic) => diagnostic.code)),
    new Set(["scheduled_job_not_executable", "binding_session_mismatch"]),
  );

  console.log(
    JSON.stringify(
      {
        fixture: "runtime-config-parity",
        brains: actualInput.runtimeConfig.brains.length,
        sessions: actualInput.runtimeConfig.sessions.length,
        scheduledJobs: actualInput.runtimeConfig.scheduledJobs.length,
        channelBindings: actualInput.runtimeConfig.channelBindings.length,
        mcpBindings: actualInput.runtimeConfig.mcpBindings.length,
        invalidCodes: invalidReport.diagnostics.map(
          (diagnostic) => diagnostic.code,
        ),
      },
      null,
      2,
    ),
  );
} finally {
  await rm(tempRoot, { force: true, recursive: true });
}

async function writeFixture(
  fixturePath: string,
  targetPath: string,
  root: string,
): Promise<void> {
  const raw = await readFixtureText(fixturePath, root);
  await writeFile(targetPath, raw);
}

async function readFixtureJson<T>(
  fixturePath: string,
  root: string,
): Promise<T> {
  return JSON.parse(await readFixtureText(fixturePath, root)) as T;
}

async function readFixtureText(
  fixturePath: string,
  root: string,
): Promise<string> {
  return (await readFile(join(fixtureRoot, fixturePath), "utf8")).replaceAll(
    "__FIXTURE_ROOT__",
    root,
  );
}

function jsonRoundTrip<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
