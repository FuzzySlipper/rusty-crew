import assert from "node:assert/strict";
import test from "node:test";

import {
  runCoordinationOperatorCli,
  type CoordinationOperatorCliIo,
} from "../src/coordination-operator-cli.js";

test("production and debug coordination clients use fixed distinct roots and routes", async () => {
  const requests: string[] = [];
  const output: string[] = [];
  const io: CoordinationOperatorCliIo = {
    readText: async (path) => {
      if (path === "/home/system/rusty-crew/config/service.env") {
        return "RUSTY_CREW_DEPLOYMENT_ROLE=production\nRUSTY_CREW_ADMIN_PORT=9347\n";
      }
      if (path === "/home/system/rusty-crew-debug/config/service.env") {
        return "RUSTY_CREW_DEPLOYMENT_ROLE=debug\nRUSTY_CREW_ADMIN_PORT=9348\n";
      }
      throw new Error(`unexpected config path ${path}`);
    },
    fetch: (async (url: URL | RequestInfo) => {
      requests.push(String(url));
      const role = String(url).includes(":9348/") ? "debug" : "production";
      return new Response(
        JSON.stringify({
          ok: true,
          data: { deploymentRole: role, agents: [] },
        }),
        { status: 200 },
      );
    }) as typeof fetch,
    write: (text) => output.push(text),
    sleep: async () => undefined,
  };

  await runCoordinationOperatorCli("production", ["list"], io);
  await runCoordinationOperatorCli("debug", ["list"], io);

  assert.deepEqual(requests, [
    "http://127.0.0.1:9347/v1/coordination/agents",
    "http://127.0.0.1:9348/v1/debug/coordination/agents",
  ]);
  assert.equal(JSON.parse(output[0] ?? "{}").data.deploymentRole, "production");
  assert.equal(JSON.parse(output[1] ?? "{}").data.deploymentRole, "debug");
});

test("fixed debug client refuses a production service environment", async () => {
  const io = {
    readText: async () =>
      "RUSTY_CREW_DEPLOYMENT_ROLE=production\nRUSTY_CREW_ADMIN_PORT=9348\n",
    fetch: async () => {
      throw new Error("fetch must not be called");
    },
    write: () => undefined,
    sleep: async () => undefined,
  } as unknown as CoordinationOperatorCliIo;

  await assert.rejects(
    runCoordinationOperatorCli("debug", ["list"], io),
    /not configured as the debug deployment/,
  );
});
