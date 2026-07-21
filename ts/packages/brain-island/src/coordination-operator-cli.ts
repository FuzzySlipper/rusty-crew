import { readFile } from "node:fs/promises";

import type { RustyCrewDeploymentRole } from "./service-config.js";

const TERMINAL_ROUND_STATUSES = new Set([
  "replied",
  "expired",
  "cancelled",
  "failed",
]);

export interface CoordinationOperatorCliIo {
  fetch: typeof fetch;
  readText(path: string): Promise<string>;
  write(text: string): void;
  sleep(ms: number): Promise<void>;
}

export async function runCoordinationOperatorCli(
  deploymentRole: RustyCrewDeploymentRole,
  args: readonly string[],
  io: CoordinationOperatorCliIo = defaultIo(),
): Promise<void> {
  const target = await loadFixedTarget(deploymentRole, io);
  const prefix =
    deploymentRole === "debug" ? "/v1/debug/coordination" : "/v1/coordination";
  const [command, ...rest] = args;

  if (command === "list" && rest.length === 0) {
    const listed = await requestJson(
      target,
      io.fetch,
      "GET",
      `${prefix}/agents`,
    );
    assertDeploymentRole(listed, deploymentRole);
    io.write(JSON.stringify(listed, null, 2));
    return;
  }

  if (command === "routes" && rest.length === 0) {
    const listed = await requestJson(
      target,
      io.fetch,
      "GET",
      `${prefix}/routes`,
    );
    assertDeploymentRole(listed, deploymentRole);
    io.write(JSON.stringify(listed, null, 2));
    return;
  }

  if (command !== "send" && command !== "round") {
    throw new Error(usage(deploymentRole));
  }
  const [toAddress, ttlSecondsText, ...messageParts] = rest;
  const ttlSeconds = Number(ttlSecondsText);
  const body = messageParts.join(" ").trim();
  if (
    !toAddress ||
    !Number.isSafeInteger(ttlSeconds) ||
    ttlSeconds < 1 ||
    ttlSeconds > 300 ||
    !body
  ) {
    throw new Error(usage(deploymentRole));
  }
  const ttlMs = ttlSeconds * 1_000;
  const started = await requestJson(
    target,
    io.fetch,
    "POST",
    `${prefix}/${command === "send" ? "messages" : "rounds"}`,
    { toAddress, ttlMs, body },
  );
  assertDeploymentRole(started, deploymentRole);

  if (command === "send") {
    io.write(JSON.stringify(started, null, 2));
    return;
  }
  const roundId = stringField(dataRecord(started), "roundId");
  const deadline = Date.now() + ttlMs + 5_000;
  let current = started;
  while (
    !TERMINAL_ROUND_STATUSES.has(stringField(dataRecord(current), "status")) &&
    Date.now() < deadline
  ) {
    await io.sleep(250);
    current = await requestJson(
      target,
      io.fetch,
      "GET",
      `${prefix}/rounds/${encodeURIComponent(roundId)}`,
    );
    assertDeploymentRole(current, deploymentRole);
  }
  io.write(JSON.stringify(current, null, 2));
}

interface FixedTarget {
  baseUrl: string;
  token?: string;
}

async function loadFixedTarget(
  deploymentRole: RustyCrewDeploymentRole,
  io: CoordinationOperatorCliIo,
): Promise<FixedTarget> {
  const root =
    deploymentRole === "debug"
      ? "/home/system/rusty-crew-debug"
      : "/home/system/rusty-crew";
  const values = parseEnv(await io.readText(`${root}/config/service.env`));
  if (values.RUSTY_CREW_DEPLOYMENT_ROLE !== deploymentRole) {
    throw new Error(
      `${root}/config/service.env is not configured as the ${deploymentRole} deployment`,
    );
  }
  const port = Number(values.RUSTY_CREW_ADMIN_PORT);
  if (!Number.isSafeInteger(port) || port < 1 || port > 65_535) {
    throw new Error(`${root}/config/service.env has an invalid admin port`);
  }
  return {
    baseUrl: `http://127.0.0.1:${port}`,
    ...(values.RUSTY_CREW_ADMIN_TOKEN === undefined
      ? {}
      : { token: values.RUSTY_CREW_ADMIN_TOKEN }),
  };
}

async function requestJson(
  target: FixedTarget,
  request: typeof fetch,
  method: "GET" | "POST",
  path: string,
  body?: Record<string, unknown>,
): Promise<unknown> {
  const response = await request(new URL(path, target.baseUrl), {
    method,
    headers: {
      ...(target.token === undefined
        ? {}
        : { authorization: `Bearer ${target.token}` }),
      ...(body === undefined ? {} : { "content-type": "application/json" }),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const parsed = JSON.parse(await response.text()) as unknown;
  if (!response.ok) {
    throw new Error(
      `coordination request failed ${response.status}: ${JSON.stringify(parsed)}`,
    );
  }
  return parsed;
}

function assertDeploymentRole(
  envelope: unknown,
  expected: RustyCrewDeploymentRole,
): void {
  const actual = stringField(dataRecord(envelope), "deploymentRole");
  if (actual !== expected) {
    throw new Error(
      `coordination client expected ${expected} deployment but service reported ${actual}`,
    );
  }
}

function dataRecord(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("coordination API returned a non-object envelope");
  }
  const data = (value as Record<string, unknown>).data;
  if (data === null || typeof data !== "object" || Array.isArray(data)) {
    throw new Error("coordination API envelope did not contain object data");
  }
  return data as Record<string, unknown>;
}

function stringField(value: Record<string, unknown>, name: string): string {
  const field = value[name];
  if (typeof field !== "string" || field.length === 0) {
    throw new Error(`coordination API data omitted ${name}`);
  }
  return field;
}

function parseEnv(text: string): Record<string, string> {
  return Object.fromEntries(
    text
      .split(/\r?\n/u)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith("#"))
      .map((line) => {
        const separator = line.indexOf("=");
        if (separator <= 0) return [line, ""];
        return [line.slice(0, separator), line.slice(separator + 1)];
      }),
  );
}

function usage(role: RustyCrewDeploymentRole): string {
  const command =
    role === "debug" ? "agent:coordination:debug" : "agent:coordination";
  return [
    `Usage: npm run ${command} -- list`,
    `       npm run ${command} -- routes`,
    `       npm run ${command} -- send <@route-or-agent-id> <ttl-seconds> <message...>`,
    `       npm run ${command} -- round <@route-or-agent-id> <ttl-seconds> <message...>`,
  ].join("\n");
}

function defaultIo(): CoordinationOperatorCliIo {
  return {
    fetch,
    readText: (path) => readFile(path, "utf8"),
    write: (text) => console.log(text),
    sleep: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
  };
}
