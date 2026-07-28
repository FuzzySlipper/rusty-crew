import { spawn } from "node:child_process";

export interface SystemdNotifierOptions {
  env?: NodeJS.ProcessEnv;
  notify?: (args: string[]) => Promise<void>;
  healthCheck?: () => Promise<boolean>;
  setIntervalFn?: typeof setInterval;
  clearIntervalFn?: typeof clearInterval;
}

export interface SystemdNotifier {
  readonly enabled: boolean;
  readonly watchdogIntervalMs?: number;
  ready(status: string): Promise<void>;
  startWatchdog(status: string): () => void;
  stopping(status: string): Promise<void>;
}

export function createSystemdNotifier(
  options: SystemdNotifierOptions = {},
): SystemdNotifier {
  const env = options.env ?? process.env;
  const notify = options.notify ?? systemdNotify;
  const healthCheck = options.healthCheck ?? (async () => true);
  const setIntervalFn = options.setIntervalFn ?? setInterval;
  const clearIntervalFn = options.clearIntervalFn ?? clearInterval;
  const enabled =
    typeof env.NOTIFY_SOCKET === "string" && env.NOTIFY_SOCKET !== "";
  const watchdogIntervalMs = enabled
    ? watchdogIntervalFromUsec(env.WATCHDOG_USEC)
    : undefined;

  return {
    enabled,
    watchdogIntervalMs,
    async ready(status) {
      if (!enabled) return;
      await notify(["--ready", `--status=${status}`]);
    },
    startWatchdog(status) {
      if (!enabled || watchdogIntervalMs === undefined) return () => undefined;
      const timer = setIntervalFn(() => {
        void healthCheck()
          .then((healthy) =>
            healthy ? notify(["WATCHDOG=1", `--status=${status}`]) : undefined,
          )
          .catch(() => undefined);
      }, watchdogIntervalMs);
      timer.unref?.();
      return () => clearIntervalFn(timer);
    },
    async stopping(status) {
      if (!enabled) return;
      await notify(["--stopping", `--status=${status}`]);
    },
  };
}

export async function systemdHealthCheck(url: string): Promise<boolean> {
  const healthUrl = new URL("/v1/admin/healthz", localHealthBaseUrl(url));
  const response = await fetch(healthUrl);
  if (!response.ok) return false;
  const body = (await response.json()) as { ok?: unknown };
  return body.ok === true;
}

export function localHealthBaseUrl(url: string): string {
  const parsed = new URL(url);
  if (parsed.hostname === "0.0.0.0" || parsed.hostname === "::") {
    parsed.hostname = "127.0.0.1";
  }
  return parsed.toString();
}

export function watchdogIntervalFromUsec(
  value: string | undefined,
): number | undefined {
  if (value === undefined || value.trim() === "") return undefined;
  const usec = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(usec) || usec <= 0) return undefined;
  return Math.max(1_000, Math.floor(usec / 2_000));
}

function systemdNotify(args: string[]): Promise<void> {
  return new Promise((resolveNotify, rejectNotify) => {
    const child = spawn("systemd-notify", systemdNotifyArguments(args), {
      stdio: "ignore",
    });
    child.once("error", rejectNotify);
    child.once("exit", (code) => {
      if (code === 0) {
        resolveNotify();
        return;
      }
      rejectNotify(new Error(`systemd-notify exited with status ${code}`));
    });
  });
}

export function systemdNotifyArguments(args: string[]): string[] {
  // Keep systemd-notify's default barrier. --no-block lets the helper exit
  // before systemd attributes the datagram to this unit, which can leave a
  // healthy Type=notify service stuck in "activating" until start timeout.
  return [...args];
}
