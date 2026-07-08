import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { startRustyCrewServiceHost } from "@rusty-crew/service-host";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-profile-create-race-"));
const port = await openPort();

let host: Awaited<ReturnType<typeof startRustyCrewServiceHost>> | undefined;
try {
  host = await startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
      RUSTY_CREW_STORAGE_BACKEND: "sqlite",
    },
  });
  const activeHost = host;

  const provider = await postJson(
    `${activeHost.url}/v1/admin/model-providers`,
    {
      alias: "default",
      displayName: "Default Local",
      protocol: "chat_completions",
      providerKind: "local",
      modelId: "deterministic",
      contextWindowTokens: 8192,
      maxOutputTokens: 512,
      temperature: 0.5,
    },
  );
  assert.equal(provider.status, 200, JSON.stringify(provider.body));

  const profileIds = Array.from(
    { length: 8 },
    (_, index) => `race-profile-${index + 1}`,
  );
  const results = await Promise.all(
    profileIds.map((profileId) =>
      postJson(`${activeHost.url}/v1/admin/control/profiles`, {
        profileId,
        displayName: `Race Profile ${profileId}`,
      }),
    ),
  );

  for (const [index, result] of results.entries()) {
    assert.equal(
      result.status,
      200,
      `profile create ${profileIds[index]} failed: ${JSON.stringify(result.body)}`,
    );
  }

  const serviceConfig = JSON.parse(
    readFileSync(join(root, "config", "service.json"), "utf8"),
  ) as {
    brains?: Array<{ profileId?: string; profile_id?: string }>;
    sessions?: Array<{ profileId?: string; profile_id?: string }>;
  };
  const brainProfileIds = new Set(
    (serviceConfig.brains ?? []).map(
      (entry) => entry.profileId ?? entry.profile_id,
    ),
  );
  const sessionProfileIds = new Set(
    (serviceConfig.sessions ?? []).map(
      (entry) => entry.profileId ?? entry.profile_id,
    ),
  );

  for (const profileId of profileIds) {
    assert.equal(
      existsSync(join(root, "config", "profiles", `${profileId}.json`)),
      true,
      `profile file missing for ${profileId}`,
    );
    assert.equal(
      brainProfileIds.has(profileId),
      true,
      `runtime brain entry missing for ${profileId}`,
    );
    assert.equal(
      sessionProfileIds.has(profileId),
      true,
      `runtime session entry missing for ${profileId}`,
    );
  }

  const registryResponse = await fetch(
    `${activeHost.url}/v1/admin/profiles/registry?limit=20`,
  );
  assert.equal(registryResponse.status, 200);
  const registryEnvelope = (await registryResponse.json()) as {
    ok?: boolean;
    data?: { items?: Array<{ profileId?: string; lifecycleStatus?: string }> };
  };
  assert.equal(registryEnvelope.ok, true, JSON.stringify(registryEnvelope));
  const registryProfileIds = new Set(
    (registryEnvelope.data?.items ?? []).map((record) => record.profileId),
  );
  for (const profileId of profileIds) {
    assert.equal(
      registryProfileIds.has(profileId),
      true,
      `registry entry missing for ${profileId}`,
    );
  }

  console.log(
    JSON.stringify(
      {
        profilesCreated: profileIds.length,
        brains: serviceConfig.brains?.length ?? 0,
        sessions: serviceConfig.sessions?.length ?? 0,
      },
      null,
      2,
    ),
  );
  console.log("admin profile create concurrency smoke passed");
} finally {
  await host?.stop();
  rmSync(root, { recursive: true, force: true });
}

async function postJson(
  url: string,
  body: unknown,
): Promise<{ status: number; body: unknown }> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    body: await response.json(),
  };
}

async function openPort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const address = server.address();
  assert.ok(address && typeof address === "object");
  const port = address.port;
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return port;
}
