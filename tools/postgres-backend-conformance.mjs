#!/usr/bin/env node
import { spawnSync } from "node:child_process";

const databaseUrlEnvKeys = [
  "RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL",
  "RUSTY_CREW_DATABASE_URL",
  "RUSTY_CREW_APP_DATABASE_URL",
];

const containerImage = process.env.RUSTY_CREW_POSTGRES_IMAGE ?? "postgres:16";
const containerUser = "rusty_crew";
const containerPassword = "rusty_crew";
const containerDatabase = "rusty_crew_test";

function usage() {
  return `
Usage:
  npm run test:postgres-backend

Environment:
  RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL  Use an existing disposable database.
  RUSTY_CREW_DATABASE_URL                   Fallback existing database URL.
  RUSTY_CREW_APP_DATABASE_URL               Fallback existing database URL.
  RUSTY_CREW_POSTGRES_HARNESS_NO_CONTAINER=1
                                           Fail instead of starting Docker/Podman.
  RUSTY_CREW_POSTGRES_IMAGE                 Container image when auto-starting.

The harness creates unique schemas through the Rust tests and drops them after
each run. Use a disposable database; do not point it at production service data.
`.trim();
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    stdio: "inherit",
    env: options.env ?? process.env,
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} exited ${result.status}`);
  }
}

function output(command, args) {
  const result = spawnSync(command, args, {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error || result.status !== 0) {
    return null;
  }
  return result.stdout.trim();
}

function configuredDatabaseUrl() {
  for (const key of databaseUrlEnvKeys) {
    const value = process.env[key]?.trim();
    if (value) {
      return { url: value, source: key };
    }
  }
  return null;
}

function findContainerRuntime() {
  for (const command of ["docker", "podman"]) {
    if (output(command, ["--version"])) {
      return command;
    }
  }
  return null;
}

function mappedPort(runtime, name) {
  const raw = output(runtime, ["port", name, "5432/tcp"]);
  if (!raw) {
    return null;
  }
  const first = raw.split(/\r?\n/).find(Boolean);
  return first?.match(/:(\d+)$/)?.[1] ?? null;
}

function waitForPostgres(runtime, name) {
  const started = Date.now();
  while (Date.now() - started < 60_000) {
    const result = spawnSync(
      runtime,
      [
        "exec",
        name,
        "pg_isready",
        "-U",
        containerUser,
        "-d",
        containerDatabase,
      ],
      { stdio: "ignore" },
    );
    if (result.status === 0) {
      return;
    }
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 1_000);
  }
  throw new Error("PostgreSQL container did not become ready within 60s");
}

function startContainer() {
  if (process.env.RUSTY_CREW_POSTGRES_HARNESS_NO_CONTAINER === "1") {
    throw new Error(
      `No database URL was provided and container startup is disabled.\n\n${usage()}`,
    );
  }
  const runtime = findContainerRuntime();
  if (!runtime) {
    throw new Error(
      `No database URL was provided and neither docker nor podman is available.\n\n${usage()}`,
    );
  }

  const name = `rusty-crew-postgres-backend-${process.pid}-${Date.now()}`;
  run(runtime, [
    "run",
    "--rm",
    "--detach",
    "--name",
    name,
    "-e",
    `POSTGRES_USER=${containerUser}`,
    "-e",
    `POSTGRES_PASSWORD=${containerPassword}`,
    "-e",
    `POSTGRES_DB=${containerDatabase}`,
    "-p",
    "127.0.0.1::5432",
    containerImage,
  ]);

  let stopped = false;
  const stop = () => {
    if (stopped) return;
    stopped = true;
    spawnSync(runtime, ["stop", name], { stdio: "inherit" });
  };
  process.once("exit", stop);
  for (const signal of ["SIGINT", "SIGTERM"]) {
    process.once(signal, () => {
      stop();
      process.exit(130);
    });
  }

  waitForPostgres(runtime, name);
  const port = mappedPort(runtime, name);
  if (!port) {
    throw new Error(`Could not resolve mapped PostgreSQL port for ${name}`);
  }
  return {
    url: `postgres://${containerUser}:${containerPassword}@127.0.0.1:${port}/${containerDatabase}`,
    source: `${runtime} container ${name}`,
    stop,
  };
}

function cargoTest(args, databaseUrl) {
  run("cargo", args, {
    env: {
      ...process.env,
      RUSTY_CREW_POSTGRES_BACKEND_DATABASE_URL: databaseUrl,
    },
  });
}

function main() {
  if (process.argv.includes("--help") || process.argv.includes("-h")) {
    console.log(usage());
    return;
  }

  const database = configuredDatabaseUrl() ?? startContainer();
  console.log(`[postgres-backend] using ${database.source}`);
  try {
    cargoTest(
      [
        "test",
        "-p",
        "rusty-crew-core-persistence",
        "--features",
        "postgres-backend",
        "postgres_migration_catalog",
      ],
      database.url,
    );
    cargoTest(
      [
        "test",
        "-p",
        "rusty-crew-core-persistence",
        "--features",
        "postgres-backend",
        "postgres_",
        "--",
        "--ignored",
        "--nocapture",
      ],
      database.url,
    );
  } finally {
    database.stop?.();
  }
}

try {
  main();
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
