import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import {
  coreConfigFacadeArtifact,
  fromCoreConfigWireRuntimeGraphPlan,
  loadNativeBridge,
  toCoreConfigWireRuntimeGraphPlanInput,
} from "@rusty-crew/native-bridge";
import type { ProfileId } from "@rusty-crew/contracts";
import type {
  NativeCreateProfileRequest,
  NativeProfileRegistryRuntimeMetadata,
  NativeRuntimeGraphPlanInput,
  NativeRuntimeConfigValidationInput,
} from "@rusty-crew/native-bridge";
import { loadProfileConfig } from "../src/profile-loading.js";
import {
  planCreateProfileWithRust,
  runtimeConfigValidationInput,
} from "../src/runtime-config-validation.js";
import { loadRustyCrewServiceConfig } from "../src/service-config.js";
import {
  loadRustyCrewRuntimeConfig,
  preflightRustyCrewRuntimeConfig,
} from "../src/service-runtime-config.js";

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
  const expectedSnakeInput = await readFixtureJson<unknown>(
    "valid/validation-input.snake.json",
    tempRoot,
  );
  assert.deepEqual(
    canonicalRuntimeValidationInput(actualInput),
    canonicalRuntimeValidationInput(expectedInput),
    "TS runtime/profile loading drifted from the shared config validation fixture",
  );
  assert.deepEqual(
    snakeCaseKeys(canonicalRuntimeValidationInput(actualInput)),
    snakeCaseKeys(canonicalRuntimeValidationInput(expectedInput)),
    "runtime config parity fixture drifted from the Rust serde snake_case shape",
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
  const profileRegistry = await readFixtureJson<
    NativeProfileRegistryRuntimeMetadata[]
  >("valid/profile-registry-runtime-metadata.camel.json", tempRoot);
  const expectedProfileRegistrySnake = await readFixtureJson<unknown[]>(
    "valid/profile-registry-runtime-metadata.snake.json",
    tempRoot,
  );
  const expectedCreateRequestSnake = await readFixtureJson<unknown>(
    "valid/create-profile-request.snake.json",
    tempRoot,
  );
  assert.deepEqual(
    snakeCaseKeys(jsonRoundTrip(createRequest)),
    expectedCreateRequestSnake,
    "create-profile parity fixture drifted from the Rust serde snake_case shape",
  );
  assert.deepEqual(
    snakeCaseKeys(jsonRoundTrip(profileRegistry)),
    expectedProfileRegistrySnake,
    "profile registry runtime metadata fixture drifted from the Rust serde snake_case shape",
  );
  assertRuntimeConfigFixtureCoverage({
    validationInputSnake: expectedSnakeInput,
    createProfilePlanInputSnake: {
      ...(expectedSnakeInput as Record<string, unknown>),
      profile_registry: expectedProfileRegistrySnake,
      request: expectedCreateRequestSnake,
    },
    manifest: await readFixtureJson<RuntimeConfigCoverageManifest>(
      "coverage-manifest.json",
      tempRoot,
    ),
  });
  const targetSourceCamel = await readFixtureJson<unknown>(
    "target/complete-source.camel.json",
    tempRoot,
  );
  const targetSourceSnake = await readFixtureJson<unknown>(
    "target/complete-source.snake.json",
    tempRoot,
  );
  const targetPlanCamel = await readFixtureJson<unknown>(
    "target/complete-plan.camel.json",
    tempRoot,
  );
  const targetPlanSnake = await readFixtureJson<unknown>(
    "target/complete-plan.snake.json",
    tempRoot,
  );
  assert.deepEqual(
    toCoreConfigWireRuntimeGraphPlanInput(targetSourceCamel),
    targetSourceSnake,
    "generated runtime graph input converter drifted from Rust wire fixture",
  );
  const actualTargetPlan = await bridge.planRuntimeGraph(
    targetSourceCamel as NativeRuntimeGraphPlanInput,
  );
  assert.equal(actualTargetPlan.accepted, true);
  assert.deepEqual(
    stripNullObjectFields({
      ...jsonRoundTrip(actualTargetPlan),
      sourceRevision: "__RUST_COMPUTED__",
    }),
    {
      ...(targetPlanCamel as Record<string, unknown>),
      sourceRevision: "__RUST_COMPUTED__",
    },
    "native runtime graph planning drifted from the Rust-owned target plan",
  );
  assert.deepEqual(
    fromCoreConfigWireRuntimeGraphPlan(targetPlanSnake),
    targetPlanCamel,
    "generated runtime graph plan converter drifted from ergonomic TS fixture",
  );
  assertCoveredFieldPaths({
    family: "RuntimeGraphPlanInput",
    value: targetSourceSnake,
    manifest: await readFixtureJson<RuntimeConfigCoverageManifest>(
      "coverage-manifest.json",
      tempRoot,
    ),
  });
  assertCoveredFieldPaths({
    family: "RuntimeGraphPlan",
    value: targetPlanSnake,
    manifest: await readFixtureJson<RuntimeConfigCoverageManifest>(
      "coverage-manifest.json",
      tempRoot,
    ),
  });
  const createPlan = await planCreateProfileWithRust({
    bridge,
    runtimeConfig,
    profiles: [profile],
    profileRegistry,
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
        coverage: {
          validationInputFields:
            coreConfigFacadeArtifact.wire_field_inventory
              .RuntimeConfigValidationInput.length,
          createProfilePlanInputFields:
            coreConfigFacadeArtifact.wire_field_inventory.CreateProfilePlanInput
              .length,
        },
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

function canonicalRuntimeValidationInput(
  input: NativeRuntimeConfigValidationInput,
): unknown {
  const value = stripNullObjectFields(jsonRoundTrip(input)) as {
    runtimeConfig: NativeRuntimeConfigValidationInput["runtimeConfig"];
    profiles: NativeRuntimeConfigValidationInput["profiles"];
  };
  value.runtimeConfig.brains.sort((left, right) =>
    left.implementationId.localeCompare(right.implementationId),
  );
  value.runtimeConfig.sessions.sort((left, right) =>
    left.sessionId.localeCompare(right.sessionId),
  );
  value.runtimeConfig.scheduledJobs.sort((left, right) =>
    left.id.localeCompare(right.id),
  );
  value.runtimeConfig.channelBindings.sort((left, right) =>
    left.bindingId.localeCompare(right.bindingId),
  );
  value.runtimeConfig.mcpBindings.sort((left, right) =>
    left.bindingId.localeCompare(right.bindingId),
  );
  value.profiles.sort((left, right) =>
    left.profileId.localeCompare(right.profileId),
  );
  return value;
}

function stripNullObjectFields(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(stripNullObjectFields);
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([, child]) => child !== null)
      .map(([key, child]) => [key, stripNullObjectFields(child)]),
  );
}

function snakeCaseKeys(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(snakeCaseKeys);
  }
  if (value === null || typeof value !== "object") {
    return value;
  }
  return Object.fromEntries(
    Object.entries(value).map(([key, entry]) => [
      camelToSnake(key),
      snakeCaseKeys(entry),
    ]),
  );
}

function camelToSnake(value: string): string {
  return value.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`);
}

interface RuntimeConfigCoverageManifest {
  formatVersion: 1;
  families: Record<
    keyof typeof coreConfigFacadeArtifact.wire_field_inventory,
    {
      exemptFieldPaths?: Array<{
        path: string;
        reason: string;
      }>;
    }
  >;
}

function assertRuntimeConfigFixtureCoverage(input: {
  validationInputSnake: unknown;
  createProfilePlanInputSnake: unknown;
  manifest: RuntimeConfigCoverageManifest;
}): void {
  assertCoveredFieldPaths({
    family: "RuntimeConfigValidationInput",
    value: input.validationInputSnake,
    manifest: input.manifest,
  });
  assertCoveredFieldPaths({
    family: "CreateProfilePlanInput",
    value: input.createProfilePlanInputSnake,
    manifest: input.manifest,
  });
}

function assertCoveredFieldPaths(input: {
  family: keyof typeof coreConfigFacadeArtifact.wire_field_inventory;
  value: unknown;
  manifest: RuntimeConfigCoverageManifest;
}): void {
  const fixturePaths = new Set(jsonFieldPaths(input.value));
  const exemptions = new Map(
    (input.manifest.families[input.family]?.exemptFieldPaths ?? []).map(
      (entry) => [entry.path, entry.reason],
    ),
  );
  const missing = coreConfigFacadeArtifact.wire_field_inventory[
    input.family
  ].filter(
    (path) =>
      !fixturePaths.has(path) && !validCoverageExemption(exemptions.get(path)),
  );
  assert.deepEqual(
    missing,
    [],
    `${input.family} fixture coverage is missing Rust-owned fields without exemptions`,
  );
}

function validCoverageExemption(reason: string | undefined): boolean {
  return reason !== undefined && reason.trim().length >= 12;
}

function jsonFieldPaths(value: unknown): string[] {
  const paths: string[] = [];
  collectJsonFieldPaths(value, "", paths);
  return paths;
}

function collectJsonFieldPaths(
  value: unknown,
  prefix: string,
  paths: string[],
): void {
  if (Array.isArray(value)) {
    for (const item of value) {
      collectJsonFieldPaths(item, `${prefix}[]`, paths);
    }
    return;
  }
  if (value === null || typeof value !== "object") {
    return;
  }
  for (const [key, child] of Object.entries(value)) {
    const path = prefix.length === 0 ? key : `${prefix}.${key}`;
    paths.push(path);
    collectJsonFieldPaths(child, path, paths);
  }
}
