import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, ProfileId, SessionId } from "@rusty-crew/contracts";

export type BrowserSessionState =
  | "starting"
  | "ready"
  | "closing"
  | "closed"
  | "crashed";

export type BrowserCloseReason =
  | "session_archived"
  | "agent_closed"
  | "idle_timeout"
  | "hard_lifetime"
  | "service_shutdown"
  | "restart"
  | "launch_failed"
  | "manual";

export interface BrowserOpenInput {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  now?: Date;
}

export interface BrowserSessionLimits {
  maxServiceSessions: number;
  maxSessionsPerAgent: number;
  maxSessionsPerProfile?: number;
  idleTimeoutMs: number;
  hardLifetimeMs: number;
  startupTimeoutMs: number;
  cdpCallTimeoutMs: number;
  maxRefs: number;
  consoleRingSize: number;
}

export interface BrowserManagerOptions {
  launcher?: BrowserLauncher;
  limits?: Partial<BrowserSessionLimits>;
  now?: () => Date;
}

export interface BrowserProcessHandle {
  pid?: number;
  killed?: boolean;
  kill(signal?: NodeJS.Signals): boolean;
}

export interface CdpConnection {
  call(
    method: string,
    params?: Record<string, unknown>,
    timeoutMs?: number,
  ): Promise<unknown>;
  close(): void | Promise<void>;
}

export interface BrowserLaunchInput extends BrowserOpenInput {
  userDataPrefix: string;
  startupTimeoutMs: number;
  cdpCallTimeoutMs: number;
  signal?: AbortSignal;
}

export interface BrowserLaunchResult {
  process: BrowserProcessHandle;
  cdp: CdpConnection;
  userDataDir: string;
  pageWebSocketUrl?: string;
}

export interface BrowserLauncher {
  launch(input: BrowserLaunchInput): Promise<BrowserLaunchResult>;
}

export interface BrowserSessionHandle {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  state: BrowserSessionState;
  generation: number;
  cdp: CdpConnection;
}

export interface BrowserRefEntry {
  ref: string;
  target: string;
  role?: string;
  name?: string;
}

export interface BrowserResolvedRef {
  sessionId: SessionId;
  generation: number;
  ref: string;
  target: string;
}

export interface BrowserSnapshot {
  sessionId: SessionId;
  generation: number;
  refs: readonly BrowserRefEntry[];
  createdAt: string;
}

export interface BrowserCleanupSummary {
  closed: number;
  reasons: Record<string, number>;
}

export interface BrowserSessionDiagnostics {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  state: BrowserSessionState;
  generation: number;
  refCount: number;
  consoleCount: number;
  pid?: number;
  currentUrl?: string;
  title?: string;
  createdAt: string;
  lastUsedAt: string;
  lastNavigationAt?: string;
  lastError?: string;
  closeReason?: BrowserCloseReason;
}

export interface BrowserManagerDiagnostics {
  activeSessions: number;
  limits: BrowserSessionLimits;
  sessions: readonly BrowserSessionDiagnostics[];
}

interface BrowserSessionRecord {
  sessionId: SessionId;
  agentId: AgentId;
  profileId: ProfileId;
  state: BrowserSessionState;
  process: BrowserProcessHandle;
  cdp: CdpConnection;
  userDataDir: string;
  pageWebSocketUrl?: string;
  generation: number;
  refs: Map<string, BrowserRefEntry>;
  console: string[];
  createdAt: Date;
  lastUsedAt: Date;
  lastNavigationAt?: Date;
  currentUrl?: string;
  title?: string;
  lastError?: string;
  closeReason?: BrowserCloseReason;
}

interface CdpResponse {
  id?: number;
  result?: unknown;
  error?: { message?: string };
}

interface CdpWebSocket {
  readonly readyState: number;
  send(data: string): void;
  close(): void;
  addEventListener(
    event: "open" | "message" | "error" | "close",
    listener: (event: unknown) => void,
  ): void;
}

interface CdpTarget {
  type?: string;
  url?: string;
  webSocketDebuggerUrl?: string;
}

const defaultLimits: BrowserSessionLimits = {
  maxServiceSessions: 8,
  maxSessionsPerAgent: 2,
  idleTimeoutMs: 10 * 60 * 1000,
  hardLifetimeMs: 60 * 60 * 1000,
  startupTimeoutMs: 15_000,
  cdpCallTimeoutMs: 15_000,
  maxRefs: 80,
  consoleRingSize: 100,
};

const wsOpen = 1;

export class BrowserSessionManager {
  readonly #launcher: BrowserLauncher;
  readonly #limits: BrowserSessionLimits;
  readonly #now: () => Date;
  readonly #sessions = new Map<SessionId, BrowserSessionRecord>();

  constructor(options: BrowserManagerOptions = {}) {
    this.#launcher = options.launcher ?? createChromiumBrowserLauncher();
    this.#limits = { ...defaultLimits, ...options.limits };
    this.#now = options.now ?? (() => new Date());
  }

  async open(
    input: BrowserOpenInput,
    signal?: AbortSignal,
  ): Promise<BrowserSessionHandle> {
    const existing = this.#sessions.get(input.sessionId);
    if (existing && existing.state === "ready" && !existing.process.killed) {
      existing.lastUsedAt = input.now ?? this.#now();
      return toHandle(existing);
    }

    this.#assertCapacity(input);
    const now = input.now ?? this.#now();
    const pending: BrowserSessionRecord = {
      sessionId: input.sessionId,
      agentId: input.agentId,
      profileId: input.profileId,
      state: "starting",
      process: dummyKilledProcess(),
      cdp: closedCdpConnection(),
      userDataDir: "",
      generation: 0,
      refs: new Map(),
      console: [],
      createdAt: now,
      lastUsedAt: now,
    };
    this.#sessions.set(input.sessionId, pending);

    try {
      const launched = await this.#launcher.launch({
        ...input,
        userDataPrefix: `rusty-crew-browser-${input.sessionId}-`,
        startupTimeoutMs: this.#limits.startupTimeoutMs,
        cdpCallTimeoutMs: this.#limits.cdpCallTimeoutMs,
        signal,
      });
      pending.process = launched.process;
      pending.cdp = launched.cdp;
      pending.userDataDir = launched.userDataDir;
      pending.pageWebSocketUrl = launched.pageWebSocketUrl;
      pending.state = "ready";
      pending.lastUsedAt = this.#now();
      return toHandle(pending);
    } catch (error) {
      pending.state = "crashed";
      pending.lastError = errorMessage(error);
      pending.closeReason = "launch_failed";
      await this.close(input.sessionId, "launch_failed");
      throw error;
    }
  }

  snapshot(sessionId: SessionId): BrowserSnapshot | undefined {
    const record = this.#sessions.get(sessionId);
    if (!record || record.state !== "ready") {
      return undefined;
    }
    return {
      sessionId,
      generation: record.generation,
      refs: [...record.refs.values()],
      createdAt: record.lastUsedAt.toISOString(),
    };
  }

  storeRefs(
    sessionId: SessionId,
    refs: readonly BrowserRefEntry[],
  ): BrowserSnapshot {
    const record = this.#requireReady(sessionId);
    record.generation += 1;
    record.refs = new Map(
      refs.slice(0, this.#limits.maxRefs).map((entry) => [entry.ref, entry]),
    );
    record.lastUsedAt = this.#now();
    return this.snapshot(sessionId)!;
  }

  invalidateRefs(sessionId: SessionId): void {
    const record = this.#sessions.get(sessionId);
    if (!record) {
      return;
    }
    record.generation += 1;
    record.refs.clear();
  }

  resolveRef(
    sessionId: SessionId,
    generation: number,
    ref: string,
  ): BrowserResolvedRef | undefined {
    const record = this.#sessions.get(sessionId);
    if (!record || record.generation !== generation) {
      return undefined;
    }
    const entry = record.refs.get(ref);
    return entry
      ? { sessionId, generation, ref, target: entry.target }
      : undefined;
  }

  recordNavigation(sessionId: SessionId, url: string, title?: string): void {
    const record = this.#requireReady(sessionId);
    record.currentUrl = url;
    record.title = title;
    record.lastNavigationAt = this.#now();
    this.invalidateRefs(sessionId);
  }

  recordConsole(sessionId: SessionId, line: string): void {
    const record = this.#requireReady(sessionId);
    record.console.push(line);
    record.console = record.console.slice(-this.#limits.consoleRingSize);
  }

  async close(
    sessionId: SessionId,
    reason: BrowserCloseReason = "manual",
  ): Promise<void> {
    const record = this.#sessions.get(sessionId);
    if (!record) {
      return;
    }
    record.state = "closing";
    record.closeReason = reason;
    record.refs.clear();
    try {
      await record.cdp.close();
    } catch (error) {
      record.lastError = errorMessage(error);
    }
    if (!record.process.killed) {
      record.process.kill("SIGTERM");
    }
    if (record.userDataDir) {
      await rm(record.userDataDir, { recursive: true, force: true }).catch(
        () => undefined,
      );
    }
    record.state = "closed";
    this.#sessions.delete(sessionId);
  }

  async closeAllForAgent(
    agentId: AgentId,
    reason: BrowserCloseReason = "agent_closed",
  ): Promise<void> {
    const sessionIds = [...this.#sessions.values()]
      .filter((record) => record.agentId === agentId)
      .map((record) => record.sessionId);
    await Promise.all(
      sessionIds.map((sessionId) => this.close(sessionId, reason)),
    );
  }

  async sweep(now: Date = this.#now()): Promise<BrowserCleanupSummary> {
    const reasons: Record<string, number> = {};
    for (const record of [...this.#sessions.values()]) {
      const idleMs = now.getTime() - record.lastUsedAt.getTime();
      const lifetimeMs = now.getTime() - record.createdAt.getTime();
      const reason =
        lifetimeMs >= this.#limits.hardLifetimeMs
          ? "hard_lifetime"
          : idleMs >= this.#limits.idleTimeoutMs
            ? "idle_timeout"
            : undefined;
      if (reason) {
        await this.close(record.sessionId, reason);
        reasons[reason] = (reasons[reason] ?? 0) + 1;
      }
    }
    return {
      closed: Object.values(reasons).reduce((sum, count) => sum + count, 0),
      reasons,
    };
  }

  diagnostics(): BrowserManagerDiagnostics {
    return {
      activeSessions: this.#sessions.size,
      limits: this.#limits,
      sessions: [...this.#sessions.values()].map((record) => ({
        sessionId: record.sessionId,
        agentId: record.agentId,
        profileId: record.profileId,
        state: record.state,
        generation: record.generation,
        refCount: record.refs.size,
        consoleCount: record.console.length,
        pid: record.process.pid,
        currentUrl: record.currentUrl,
        title: record.title,
        createdAt: record.createdAt.toISOString(),
        lastUsedAt: record.lastUsedAt.toISOString(),
        lastNavigationAt: record.lastNavigationAt?.toISOString(),
        lastError: record.lastError,
        closeReason: record.closeReason,
      })),
    };
  }

  #assertCapacity(input: BrowserOpenInput): void {
    const active = [...this.#sessions.values()].filter(
      (record) => record.state !== "closed" && record.state !== "closing",
    );
    if (active.length >= this.#limits.maxServiceSessions) {
      throw new Error("browser session service limit reached");
    }
    if (
      active.filter((record) => record.agentId === input.agentId).length >=
      this.#limits.maxSessionsPerAgent
    ) {
      throw new Error("browser session agent limit reached");
    }
    if (
      this.#limits.maxSessionsPerProfile !== undefined &&
      active.filter((record) => record.profileId === input.profileId).length >=
        this.#limits.maxSessionsPerProfile
    ) {
      throw new Error("browser session profile limit reached");
    }
  }

  #requireReady(sessionId: SessionId): BrowserSessionRecord {
    const record = this.#sessions.get(sessionId);
    if (!record || record.state !== "ready") {
      throw new Error(`browser session is not ready: ${sessionId}`);
    }
    return record;
  }
}

export function createChromiumBrowserLauncher(
  config: {
    browserBinaryPath?: string;
    fetchImpl?: typeof fetch;
  } = {},
): BrowserLauncher {
  const fetchImpl = config.fetchImpl ?? fetch;
  return {
    async launch(input) {
      const userDataDir = await mkdtemp(join(tmpdir(), input.userDataPrefix));
      const processHandle = spawn(
        config.browserBinaryPath ??
          process.env.RUSTY_CREW_CHROMIUM_PATH ??
          "chromium",
        [
          "--headless=new",
          "--remote-debugging-port=0",
          `--user-data-dir=${userDataDir}`,
          "--disable-gpu",
          "--no-first-run",
          "about:blank",
        ],
        { stdio: ["ignore", "pipe", "pipe"] },
      );
      try {
        const port = await debuggerPort(userDataDir, input.startupTimeoutMs);
        const targets = await fetchJson<readonly CdpTarget[]>(
          fetchImpl,
          `http://127.0.0.1:${port}/json`,
          input.signal,
        );
        const target =
          targets.find(
            (entry) => entry.type === "page" && entry.url === "about:blank",
          ) ??
          targets.find((entry) => entry.type === "page") ??
          targets.find((entry) => entry.webSocketDebuggerUrl !== undefined);
        if (!target?.webSocketDebuggerUrl) {
          throw new Error("Chromium did not expose a page websocket");
        }
        const cdp = await CdpWebSocketConnection.connect(
          target.webSocketDebuggerUrl,
          input.cdpCallTimeoutMs,
        );
        await cdp.call("Page.enable", {});
        await cdp.call("Runtime.enable", {});
        return {
          process: processHandle,
          cdp,
          userDataDir,
          pageWebSocketUrl: target.webSocketDebuggerUrl,
        };
      } catch (error) {
        processHandle.kill("SIGTERM");
        await rm(userDataDir, { recursive: true, force: true }).catch(
          () => undefined,
        );
        throw error;
      }
    },
  };
}

function toHandle(record: BrowserSessionRecord): BrowserSessionHandle {
  return {
    sessionId: record.sessionId,
    agentId: record.agentId,
    profileId: record.profileId,
    state: record.state,
    generation: record.generation,
    cdp: record.cdp,
  };
}

async function debuggerPort(
  userDataDir: string,
  timeoutMs: number,
): Promise<number> {
  const path = join(userDataDir, "DevToolsActivePort");
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const text = await readFile(path, "utf8").catch(() => undefined);
    const port = Number.parseInt(text?.split("\n")[0] ?? "", 10);
    if (Number.isFinite(port)) {
      return port;
    }
    await delay(100);
  }
  throw new Error("timed out waiting for Chromium DevToolsActivePort");
}

async function fetchJson<T>(
  fetchImpl: typeof fetch,
  url: string,
  signal?: AbortSignal,
): Promise<T> {
  const response = await fetchImpl(url, { signal });
  if (!response.ok) {
    throw new Error(`HTTP ${response.status}`);
  }
  return (await response.json()) as T;
}

class CdpWebSocketConnection implements CdpConnection {
  readonly #socket: CdpWebSocket;
  readonly #defaultTimeoutMs: number;
  #nextId = 1;
  readonly #pending = new Map<
    number,
    {
      resolve: (value: unknown) => void;
      reject: (error: Error) => void;
    }
  >();

  private constructor(socket: CdpWebSocket, defaultTimeoutMs: number) {
    this.#socket = socket;
    this.#defaultTimeoutMs = defaultTimeoutMs;
    socket.addEventListener("message", (event) => {
      this.#onMessage(event);
    });
  }

  static connect(
    url: string,
    defaultTimeoutMs: number,
  ): Promise<CdpWebSocketConnection> {
    const WebSocketCtor = globalThis.WebSocket as unknown as {
      new (url: string): CdpWebSocket;
    };
    const socket = new WebSocketCtor(url);
    return new Promise((resolve, reject) => {
      socket.addEventListener("open", () => {
        resolve(new CdpWebSocketConnection(socket, defaultTimeoutMs));
      });
      socket.addEventListener("error", () => {
        reject(new Error("CDP websocket failed to open"));
      });
    });
  }

  call(
    method: string,
    params: Record<string, unknown> = {},
    timeoutMs = this.#defaultTimeoutMs,
  ): Promise<unknown> {
    if (this.#socket.readyState !== wsOpen) {
      throw new Error("CDP websocket is not open");
    }
    const id = this.#nextId;
    this.#nextId += 1;
    this.#socket.send(JSON.stringify({ id, method, params }));
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`CDP method timed out: ${method}`));
      }, timeoutMs);
      this.#pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
    });
  }

  close(): void {
    this.#socket.close();
    for (const pending of this.#pending.values()) {
      pending.reject(new Error("CDP websocket closed"));
    }
    this.#pending.clear();
  }

  #onMessage(event: unknown): void {
    const data = eventData(event);
    if (!data) {
      return;
    }
    const message = JSON.parse(data) as CdpResponse;
    if (message.id === undefined) {
      return;
    }
    const pending = this.#pending.get(message.id);
    if (!pending) {
      return;
    }
    this.#pending.delete(message.id);
    if (message.error) {
      pending.reject(new Error(message.error.message ?? "CDP error"));
    } else {
      pending.resolve(message.result ?? {});
    }
  }
}

function dummyKilledProcess(): BrowserProcessHandle {
  return {
    killed: true,
    kill: () => false,
  };
}

function closedCdpConnection(): CdpConnection {
  return {
    async call() {
      throw new Error("CDP connection is closed");
    },
    close() {
      return undefined;
    },
  };
}

function eventData(event: unknown): string | undefined {
  const data = (event as { data?: unknown }).data;
  return typeof data === "string" ? data : undefined;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
