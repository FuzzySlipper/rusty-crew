import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import type {
  AgentId,
  ProfileId,
  RuntimeActivityBegin,
  RuntimeActivityFinish,
  SessionId,
} from "@rusty-crew/contracts";
import {
  defaultLocalCodeResourcePolicy,
  terminalTool,
  type LocalToolContext,
  type LocalToolProcessResult,
} from "../src/local-code-tools.js";

test(
  "terminal settles after its direct shell exits while an intentional daemon keeps running",
  { skip: process.platform === "win32" },
  async () => {
    const fixture = await createFixture();
    const activityBegins: RuntimeActivityBegin[] = [];
    const activityFinishes: RuntimeActivityFinish[] = [];
    let daemonPid: number | undefined;
    try {
      const result = await terminalTool(
        fixture.context({ activityBegins, activityFinishes }),
      ).execute("background-daemon", {
        command: `sleep 30 & echo $! > ${shellQuote(fixture.pidPath)}; printf ready`,
        timeoutMs: 2_000,
      });
      daemonPid = Number.parseInt(await readFile(fixture.pidPath, "utf8"), 10);
      const details = result.details as LocalToolProcessResult;

      assert.equal(details.exitCode, 0);
      assert.equal(details.timedOut, false);
      assert.equal(details.stdout, "ready");
      assert.equal(processExists(daemonPid), true);
      assert.equal(activityBegins.length, 1);
      assert.equal(activityFinishes.length, 1);
      assert.equal(activityFinishes[0]?.status, "completed");
    } finally {
      if (daemonPid !== undefined) killProcess(daemonPid);
      await fixture.cleanup();
    }
  },
);

test(
  "terminal timeout terminates background descendants in its process group",
  { skip: process.platform === "win32" },
  async () => {
    const fixture = await createFixture();
    let descendantPid: number | undefined;
    try {
      const resultPromise = terminalTool(fixture.context()).execute(
        "timeout-tree",
        {
          command: `trap '' TERM; sleep 30 & echo $! > ${shellQuote(fixture.pidPath)}; wait`,
          timeoutMs: 100,
        },
      );
      descendantPid = await waitForPid(fixture.pidPath);
      const result = await resultPromise;
      const details = result.details as LocalToolProcessResult;

      assert.equal(details.timedOut, true);
      await waitForProcessStop(descendantPid);
      assert.equal(processIsRunning(descendantPid), false);
    } finally {
      if (descendantPid !== undefined) killProcess(descendantPid);
      await fixture.cleanup();
    }
  },
);

test(
  "terminal cancellation terminates background descendants in its process group",
  { skip: process.platform === "win32" },
  async () => {
    const fixture = await createFixture();
    const controller = new AbortController();
    let descendantPid: number | undefined;
    try {
      const resultPromise = terminalTool(fixture.context()).execute(
        "cancel-tree",
        {
          command: `trap '' TERM; sleep 30 & echo $! > ${shellQuote(fixture.pidPath)}; wait`,
          timeoutMs: 2_000,
        },
        controller.signal,
      );
      descendantPid = await waitForPid(fixture.pidPath);
      controller.abort();

      await assert.rejects(resultPromise, { name: "AbortError" });
      await waitForProcessStop(descendantPid);
      assert.equal(processIsRunning(descendantPid), false);
    } finally {
      if (descendantPid !== undefined) killProcess(descendantPid);
      await fixture.cleanup();
    }
  },
);

async function createFixture(): Promise<{
  pidPath: string;
  context: (input?: {
    activityBegins: RuntimeActivityBegin[];
    activityFinishes: RuntimeActivityFinish[];
  }) => LocalToolContext;
  cleanup: () => Promise<void>;
}> {
  const workdir = await mkdtemp(join(tmpdir(), "rusty-crew-process-tree-"));
  return {
    pidPath: join(workdir, "child.pid"),
    context: (input) => ({
      workdir,
      maxReadBytes: 1_024,
      maxSearchFileBytes: 1_024,
      maxCommandOutputBytes: 4_096,
      commandTimeoutMs: 2_000,
      resourcePolicy: defaultLocalCodeResourcePolicy,
      ...(input === undefined
        ? {}
        : {
            runtimeActivity: {
              bridge: {
                beginRuntimeActivity: async (activity) => {
                  input.activityBegins.push(activity);
                  return activity as never;
                },
                finishRuntimeActivity: async (activity) => {
                  input.activityFinishes.push(activity);
                  return activity as never;
                },
              },
              wakeId: "wake-process-tree",
              session: {
                agentId: "agent-process-tree" as AgentId,
                profileId: "profile-process-tree" as ProfileId,
                sessionId: "session-process-tree" as SessionId,
              },
            },
          }),
    }),
    cleanup: () => rm(workdir, { force: true, recursive: true }),
  };
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'\\''`)}'`;
}

async function waitForPid(path: string): Promise<number> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const value = await readFile(path, "utf8").catch(() => undefined);
    if (value !== undefined) return Number.parseInt(value, 10);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
  throw new Error(`timed out waiting for pid file ${path}`);
}

async function waitForProcessStop(pid: number): Promise<void> {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (!processIsRunning(pid)) return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

function processIsRunning(pid: number): boolean {
  if (process.platform === "linux") {
    try {
      const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
      const state = stat.slice(stat.lastIndexOf(")") + 2).split(" ", 1)[0];
      return state !== "Z";
    } catch {
      return false;
    }
  }
  return processExists(pid);
}

function processExists(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function killProcess(pid: number): void {
  try {
    process.kill(pid, "SIGKILL");
  } catch {
    // The process already exited.
  }
}
