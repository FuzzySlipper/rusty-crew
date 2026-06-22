import { resolve } from "node:path";
import type { RustyCrewServiceConfig } from "./service-config.js";
import { loadRustyCrewServiceConfig } from "./service-config.js";
import { loadRustyCrewRuntimeConfig } from "./service-runtime-config.js";

export type CronCliCommand =
  | { kind: "list"; configPath?: string }
  | { kind: "tick"; baseUrl?: string; token?: string }
  | { kind: "run"; jobId: string; baseUrl?: string; token?: string }
  | { kind: "pause"; jobId: string; baseUrl?: string; token?: string }
  | {
      kind: "resume";
      jobId: string;
      nextDueAt: string;
      baseUrl?: string;
      token?: string;
    }
  | {
      kind: "runs";
      jobId?: string;
      limit?: number;
      baseUrl?: string;
      token?: string;
    };

export interface CronCliOptions {
  args: readonly string[];
  env?: NodeJS.ProcessEnv;
  write?: (text: string) => void;
}

export function parseCronArgs(args: readonly string[]): CronCliCommand {
  const [command, ...rest] = args;
  switch (command) {
    case "list":
      return optionalFields(
        { kind: "list" },
        { configPath: option(rest, "--config") },
      );
    case "tick":
      return controlFields({ kind: "tick" }, rest);
    case "run": {
      const jobId = requiredOption(rest, "--job");
      return controlFields({ kind: "run", jobId }, rest);
    }
    case "pause": {
      const jobId = requiredOption(rest, "--job");
      return controlFields({ kind: "pause", jobId }, rest);
    }
    case "resume": {
      const jobId = requiredOption(rest, "--job");
      const nextDueAt = requiredOption(rest, "--next-due-at");
      return controlFields({ kind: "resume", jobId, nextDueAt }, rest);
    }
    case "runs":
      return optionalFields(
        { kind: "runs" },
        {
          jobId: option(rest, "--job"),
          limit: numericOption(rest, "--limit"),
          baseUrl: option(rest, "--base-url"),
          token: option(rest, "--token"),
        },
      );
    default:
      throw new Error(usage());
  }
}

function controlFields<T extends object>(
  command: T,
  args: readonly string[],
): T & { baseUrl?: string; token?: string } {
  return optionalFields(command, {
    baseUrl: option(args, "--base-url"),
    token: option(args, "--token"),
  });
}

function optionalFields<T extends object, U extends Record<string, unknown>>(
  base: T,
  fields: U,
): T & { [K in keyof U]?: Exclude<U[K], undefined> } {
  const definedEntries = Object.entries(fields).filter(
    (entry): entry is [string, Exclude<unknown, undefined>] =>
      entry[1] !== undefined,
  );
  return Object.assign({}, base, Object.fromEntries(definedEntries));
}

export async function runRustyCrewCronCli(
  options: CronCliOptions,
): Promise<void> {
  const env = options.env ?? process.env;
  const write = options.write ?? ((text) => console.log(text));
  const command = parseCronArgs(options.args);

  if (command.kind === "list") {
    const serviceConfig = serviceConfigForCli(env, command.configPath);
    const runtimeConfig = await loadRustyCrewRuntimeConfig(serviceConfig);
    write(
      JSON.stringify(
        {
          jobs: runtimeConfig.scheduledJobs.map((job) => ({
            id: job.id,
            schedule: job.schedule,
            shape: job.shape,
            targetSessionId: job.targetSessionId,
            deliveryChannelId: job.deliveryChannelId,
          })),
        },
        null,
        2,
      ),
    );
    return;
  }

  if (command.kind === "runs") {
    const baseUrl =
      command.baseUrl ??
      env.RUSTY_CREW_ADMIN_BASE_URL ??
      "http://127.0.0.1:9347";
    const token = command.token ?? env.RUSTY_CREW_ADMIN_TOKEN;
    const runs = await getSchedulerRuns(
      baseUrl,
      token,
      optionalFields(
        {},
        {
          jobId: command.jobId,
          limit: command.limit,
        },
      ),
    );
    write(JSON.stringify(runs, null, 2));
    return;
  }

  const baseUrl =
    command.baseUrl ?? env.RUSTY_CREW_ADMIN_BASE_URL ?? "http://127.0.0.1:9347";
  const token = command.token ?? env.RUSTY_CREW_ADMIN_TOKEN;
  const result = await postAdminControl(baseUrl, token, controlPath(command), {
    reason: "cron CLI",
    ...(command.kind === "resume" ? { nextDueAt: command.nextDueAt } : {}),
  });
  write(JSON.stringify(result, null, 2));
}

async function getSchedulerRuns(
  baseUrl: string,
  token: string | undefined,
  query: { jobId?: string; limit?: number },
): Promise<unknown> {
  const url = new URL("/v1/admin/scheduler/runs", baseUrl);
  if (query.jobId !== undefined) url.searchParams.set("jobId", query.jobId);
  if (query.limit !== undefined) {
    url.searchParams.set("limit", String(query.limit));
  }
  const response = await fetch(url, {
    method: "GET",
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
    },
  });
  const parsed = JSON.parse(await response.text()) as unknown;
  if (!response.ok) {
    throw new Error(
      `cron runs failed ${response.status}: ${JSON.stringify(parsed)}`,
    );
  }
  return parsed;
}

function serviceConfigForCli(
  env: NodeJS.ProcessEnv,
  configPath: string | undefined,
): RustyCrewServiceConfig {
  const serviceConfig = loadRustyCrewServiceConfig({
    ...env,
    RUSTY_CREW_ADMIN_AUTH_MODE:
      env.RUSTY_CREW_ADMIN_AUTH_MODE ??
      (env.RUSTY_CREW_ADMIN_TOKEN ? undefined : "none"),
  });
  if (configPath !== undefined) {
    serviceConfig.paths.serviceConfigFile = resolve(configPath);
  }
  return serviceConfig;
}

function controlPath(
  command: Exclude<CronCliCommand, { kind: "list" } | { kind: "runs" }>,
): string {
  switch (command.kind) {
    case "tick":
      return "/v1/admin/control/scheduler/tick";
    case "run":
      return `/v1/admin/control/scheduler/jobs/${encodeURIComponent(command.jobId)}/run`;
    case "pause":
      return `/v1/admin/control/scheduler/jobs/${encodeURIComponent(command.jobId)}/pause`;
    case "resume":
      return `/v1/admin/control/scheduler/jobs/${encodeURIComponent(command.jobId)}/resume`;
  }
}

async function postAdminControl(
  baseUrl: string,
  token: string | undefined,
  path: string,
  body: unknown,
): Promise<unknown> {
  const response = await fetch(new URL(path, baseUrl), {
    method: "POST",
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      "content-type": "application/json",
    },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  const parsed = text ? JSON.parse(text) : null;
  if (!response.ok) {
    throw new Error(
      `cron control failed ${response.status}: ${JSON.stringify(parsed)}`,
    );
  }
  return parsed;
}

function option(args: readonly string[], name: string): string | undefined {
  const index = args.indexOf(name);
  if (index < 0) return undefined;
  const value = args[index + 1];
  if (!value || value.startsWith("--")) {
    throw new Error(`${name} requires a path value`);
  }
  return value;
}

function requiredOption(args: readonly string[], name: string): string {
  const value = option(args, name);
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function numericOption(
  args: readonly string[],
  name: string,
): number | undefined {
  const raw = option(args, name);
  if (raw === undefined) return undefined;
  const value = Number.parseInt(raw, 10);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value;
}

function usage(): string {
  return [
    "Usage:",
    "  npm run cron -- list [--config <service.json>]",
    "  npm run cron -- tick [--base-url <url>] [--token <token>]",
    "  npm run cron -- run --job <id> [--base-url <url>] [--token <token>]",
    "  npm run cron -- pause --job <id> [--base-url <url>] [--token <token>]",
    "  npm run cron -- resume --job <id> --next-due-at <iso> [--base-url <url>] [--token <token>]",
    "  npm run cron -- runs [--job <id>] [--limit <n>] [--base-url <url>] [--token <token>]",
  ].join("\n");
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await runRustyCrewCronCli({ args: process.argv.slice(2) }).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(2);
  });
}
