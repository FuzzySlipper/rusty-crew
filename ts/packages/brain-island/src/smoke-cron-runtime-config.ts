import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { ScheduledJobSummary, SessionId } from "@rusty-crew/contracts";
import {
  loadRustyCrewServiceConfig,
  parseCronArgs,
  runRustyCrewCronCli,
} from "./index.js";
import {
  loadRustyCrewRuntimeConfig,
  registerConfiguredScheduledJobs,
} from "./service-runtime-config.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-cron-config-"));

try {
  const configDir = join(root, "config");
  mkdirSync(configDir, { recursive: true });
  const profilesDir = join(configDir, "profiles");
  mkdirSync(profilesDir, { recursive: true });
  writeFileSync(
    join(profilesDir, "cron-profile.json"),
    JSON.stringify(
      {
        profileId: "cron-profile",
        displayName: "Cron Profile",
        modelConfig: { provider: "local", modelName: "deterministic" },
        toolPolicy: { requestedTools: [] },
      },
      null,
      2,
    ),
  );
  const configPath = join(configDir, "service.json");
  writeFileSync(
    configPath,
    JSON.stringify(
      {
        profilesDir,
        brains: [],
        sessions: [
          {
            sessionId: "cron-session",
            agentId: "cron-agent",
            profileId: "cron-profile",
            kind: "full",
          },
        ],
        scheduledJobs: [
          {
            id: "cron-wake",
            schedule: "*/15 9-10 * * 1-5",
            shape: "session_wake",
            targetSessionId: "cron-session",
            deliveryChannelId: "ops",
          },
        ],
      },
      null,
      2,
    ),
  );

  assert.deepEqual(parseCronArgs(["list", "--config", configPath]), {
    kind: "list",
    configPath,
  });
  assert.deepEqual(parseCronArgs(["run", "--job", "cron-wake"]), {
    kind: "run",
    jobId: "cron-wake",
  });
  assert.deepEqual(
    parseCronArgs([
      "resume",
      "--job",
      "cron-wake",
      "--next-due-at",
      "2026-06-21T00:00:00Z",
    ]),
    {
      kind: "resume",
      jobId: "cron-wake",
      nextDueAt: "2026-06-21T00:00:00Z",
    },
  );
  assert.deepEqual(parseCronArgs(["runs", "--job", "cron-wake"]), {
    kind: "runs",
    jobId: "cron-wake",
  });

  const serviceConfig = loadRustyCrewServiceConfig({
    RUSTY_CREW_DATA_DIR: root,
    RUSTY_CREW_ADMIN_AUTH_MODE: "none",
  });
  const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
  assert.equal(runtimeConfig.scheduledJobs.length, 1);
  assert.equal(runtimeConfig.scheduledJobs[0]?.id, "cron-wake");

  const registered: Array<{
    jobId: string;
    targetSessionId: SessionId;
    firstDueAt: string;
  }> = [];
  const result = await registerConfiguredScheduledJobs({
    runtimeConfig,
    now: () => "2026-06-15T09:01:10.000Z",
    bridge: {
      registerScheduledWakeJob: async (input) => {
        registered.push(input);
        return {
          jobId: input.jobId,
          jobKind: "runtime.wake.session",
          targetSessionId: input.targetSessionId,
          nextDueAt: input.firstDueAt,
          status: "active",
          createdAt: "2026-06-15T09:01:10.000Z",
          updatedAt: "2026-06-15T09:01:10.000Z",
        } satisfies ScheduledJobSummary;
      },
    },
  });
  assert.equal(result.registered, 1);
  assert.equal(registered[0]?.firstDueAt, "2026-06-15T09:15:00.000Z");

  const outputs: string[] = [];
  await runRustyCrewCronCli({
    args: ["list", "--config", configPath],
    env: { RUSTY_CREW_DATA_DIR: root, RUSTY_CREW_ADMIN_AUTH_MODE: "none" },
    write: (text) => outputs.push(text),
  });
  assert.match(outputs[0] ?? "", /cron-wake/);
  assert.match(outputs[0] ?? "", /session_wake/);

  const originalFetch = globalThis.fetch;
  globalThis.fetch = (async (input) => {
    const url = new URL(String(input));
    assert.equal(url.pathname, "/v1/admin/scheduler/runs");
    assert.equal(url.searchParams.get("jobId"), "cron-wake");
    assert.equal(url.searchParams.get("limit"), "2");
    return new Response(
      JSON.stringify({
        ok: true,
        data: {
          runs: [
            {
              runId: "scheduled:cron-wake:1",
              jobId: "cron-wake",
              status: "completed",
            },
          ],
        },
      }),
      { status: 200 },
    );
  }) as typeof fetch;
  try {
    await runRustyCrewCronCli({
      args: ["runs", "--job", "cron-wake", "--limit", "2"],
      write: (text) => outputs.push(text),
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
  assert.match(outputs[1] ?? "", /scheduled:cron-wake:1/);

  writeFileSync(
    configPath,
    JSON.stringify(
      {
        scheduledJobs: [
          {
            id: "script-job",
            schedule: "* * * * *",
            shape: "script_only",
            script: "printf no",
          },
        ],
      },
      null,
      2,
    ),
  );
  await assert.rejects(
    () => loadRustyCrewRuntimeConfig(serviceConfig),
    /not executable in Rusty Crew v1/,
  );

  console.log(
    JSON.stringify(
      {
        listed: runtimeConfig.scheduledJobs.map((job) => job.id),
        registeredFirstDueAt: registered[0]?.firstDueAt,
        runsCommand: JSON.parse(outputs[1] ?? "{}").data.runs[0].runId,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
