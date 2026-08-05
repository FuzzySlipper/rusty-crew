import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { loadProfileConfig, ProfileLoadError } from "../src/profile-loading.js";

test("profile delivery policy defaults explicitly and accepts serial delivery", async () => {
  const profilesDir = await mkdtemp(
    join(tmpdir(), "rusty-crew-delivery-policy-"),
  );
  try {
    await writeFile(
      join(profilesDir, "default-policy.json"),
      JSON.stringify({
        profileId: "default-policy",
        modelConfig: { provider: "local", modelName: "deterministic" },
      }),
    );
    await writeFile(
      join(profilesDir, "serial-policy.json"),
      JSON.stringify({
        profileId: "serial-policy",
        modelConfig: { provider: "local", modelName: "deterministic" },
        externalMessageDeliveryPolicy: "serial_next_turn",
      }),
    );

    assert.equal(
      (await loadProfileConfig(profilesDir, "default-policy" as never))
        .externalMessageDeliveryPolicy,
      "immediate_steer",
    );
    assert.equal(
      (await loadProfileConfig(profilesDir, "serial-policy" as never))
        .externalMessageDeliveryPolicy,
      "serial_next_turn",
    );
  } finally {
    await rm(profilesDir, { recursive: true, force: true });
  }
});

test("profile delivery policy rejects unknown values instead of resetting them", async () => {
  const profilesDir = await mkdtemp(
    join(tmpdir(), "rusty-crew-delivery-policy-invalid-"),
  );
  try {
    await writeFile(
      join(profilesDir, "invalid-policy.json"),
      JSON.stringify({
        profileId: "invalid-policy",
        modelConfig: { provider: "local", modelName: "deterministic" },
        externalMessageDeliveryPolicy: "queue_everything",
      }),
    );

    await assert.rejects(
      loadProfileConfig(profilesDir, "invalid-policy" as never),
      (error: unknown) =>
        error instanceof ProfileLoadError &&
        error.code === "invalid_profile_config" &&
        error.message.includes("externalMessageDeliveryPolicy"),
    );
  } finally {
    await rm(profilesDir, { recursive: true, force: true });
  }
});
