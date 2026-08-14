import assert from "node:assert/strict";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { loadRustyCrewServiceConfig } from "../src/service-config.js";
import { preflightRustyCrewRuntimeConfig } from "../src/service-runtime-config.js";

test("preflight omits a stale materialized MCP binding without rejecting unrelated config", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-crew-stale-mcp-"));
  try {
    const serviceConfig = loadRustyCrewServiceConfig({
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_AUTH_MODE: "none",
    });
    await mkdir(serviceConfig.paths.configDir, { recursive: true });
    await writeFile(
      serviceConfig.paths.serviceConfigFile,
      `${JSON.stringify(
        {
          profilesDir: join(root, "profiles"),
          brains: [],
          sessions: [],
          scheduledJobs: [],
          channelBindings: [],
          mcpBindings: [
            {
              bindingId: "legacy-orphan",
              adapterId: "mcp",
              agentId: "reviewer",
              sessionId: "reviewer-session",
              profileId: "reviewer",
              serverNames: ["den"],
              status: "active",
            },
          ],
        },
        null,
        2,
      )}\n`,
    );

    const report = await preflightRustyCrewRuntimeConfig({ serviceConfig });

    assert.equal(report.ok, true);
    assert.equal(report.summary.mcpBindings, 0);
    assert.deepEqual(
      report.diagnostics.map(({ severity, code, path }) => ({
        severity,
        code,
        path,
      })),
      [
        {
          severity: "warning",
          code: "stale_mcp_binding_omitted",
          path: "mcpBindings[0].sessionId",
        },
      ],
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
