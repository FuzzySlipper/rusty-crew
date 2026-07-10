import type {
  GitHubGateTerminalEvent,
  GitHubGateTerminalReceipt,
} from "@rusty-crew/contracts";

export interface ReviewGitHubGateBridge {
  consumeGitHubGateTerminalEvent(
    event: GitHubGateTerminalEvent,
  ): Promise<GitHubGateTerminalReceipt>;
  gitHubGateEventCursor(): Promise<number>;
  recoverGitHubGateWakes(): Promise<number>;
}

export interface ReviewGitHubGateEventConsumerStatus {
  state: "connected" | "degraded" | "stopped";
  cursor: number;
  acceptedEvents: number;
  scheduledWakes: number;
  duplicateEvents: number;
  ignoredEvents: number;
  lastError?: string;
}

export class ReviewGitHubGateEventConsumer {
  readonly #baseUrl: URL;
  readonly #projectId: string;
  readonly #bridge: ReviewGitHubGateBridge;
  readonly #fetch: typeof fetch;
  readonly #waitMs: number;
  readonly #status: ReviewGitHubGateEventConsumerStatus = {
    state: "stopped",
    cursor: 0,
    acceptedEvents: 0,
    scheduledWakes: 0,
    duplicateEvents: 0,
    ignoredEvents: 0,
  };

  constructor(options: {
    baseUrl: URL;
    projectId: string;
    bridge: ReviewGitHubGateBridge;
    fetch?: typeof fetch;
    waitMs?: number;
  }) {
    this.#baseUrl = options.baseUrl;
    this.#projectId = options.projectId.trim();
    this.#bridge = options.bridge;
    this.#fetch = options.fetch ?? fetch;
    this.#waitMs = Math.max(0, Math.min(options.waitMs ?? 45_000, 50_000));
  }

  status(): ReviewGitHubGateEventConsumerStatus {
    return { ...this.#status };
  }

  async hydrate(): Promise<number> {
    this.#status.cursor = await this.#bridge.gitHubGateEventCursor();
    const recovered = await this.#bridge.recoverGitHubGateWakes();
    this.#status.state = "connected";
    delete this.#status.lastError;
    return recovered;
  }

  async pollOnce(signal?: AbortSignal): Promise<GitHubGateTerminalReceipt[]> {
    const url = new URL(
      `/v1/projects/${encodeURIComponent(this.#projectId)}/review/github-check-gate-events`,
      this.#baseUrl,
    );
    url.searchParams.set("after_id", String(this.#status.cursor));
    url.searchParams.set("limit", "100");
    url.searchParams.set("wait_ms", String(this.#waitMs));
    try {
      const response = await this.#fetch(url, { signal });
      if (!response.ok) {
        throw new Error(
          `Review terminal events returned HTTP ${response.status}`,
        );
      }
      const page = parseEventPage(await response.json());
      const receipts: GitHubGateTerminalReceipt[] = [];
      for (const event of page.events) {
        const receipt =
          await this.#bridge.consumeGitHubGateTerminalEvent(event);
        receipts.push(receipt);
        this.#status.cursor = Math.max(this.#status.cursor, receipt.cursor);
        this.#status.acceptedEvents += 1;
        if (receipt.wakeScheduled) this.#status.scheduledWakes += 1;
        if (receipt.duplicate) this.#status.duplicateEvents += 1;
        if (receipt.ignoredReason !== undefined)
          this.#status.ignoredEvents += 1;
      }
      this.#status.cursor = Math.max(this.#status.cursor, page.nextCursor);
      this.#status.state = "connected";
      delete this.#status.lastError;
      return receipts;
    } catch (error) {
      if (signal?.aborted === true) throw error;
      this.#status.state = "degraded";
      this.#status.lastError =
        error instanceof Error ? error.message : String(error);
      return [];
    }
  }

  async run(signal: AbortSignal): Promise<void> {
    try {
      await this.hydrate();
    } catch (error) {
      this.#status.state = "degraded";
      this.#status.lastError =
        error instanceof Error ? error.message : String(error);
    }
    while (!signal.aborted) {
      await this.pollOnce(signal);
      if (this.#status.state === "degraded") {
        await new Promise((resolve) => setTimeout(resolve, 5_000));
      }
    }
    this.#status.state = "stopped";
  }
}

function parseEventPage(value: unknown): {
  events: GitHubGateTerminalEvent[];
  nextCursor: number;
} {
  if (!isRecord(value) || !Array.isArray(value.events)) {
    throw new Error("Review terminal event page is invalid");
  }
  return {
    events: value.events.map(parseTerminalEvent),
    nextCursor: numberField(value, "next_cursor"),
  };
}

function parseTerminalEvent(value: unknown): GitHubGateTerminalEvent {
  if (!isRecord(value)) throw new Error("Review terminal event is invalid");
  return {
    eventId: numberField(value, "id"),
    gateId: numberField(value, "gate_id"),
    projectId: stringField(
      value,
      "project_id",
    ) as GitHubGateTerminalEvent["projectId"],
    taskId: String(value.task_id) as GitHubGateTerminalEvent["taskId"],
    commitSha: stringField(value, "commit_sha"),
    status: stringField(value, "status") as GitHubGateTerminalEvent["status"],
    terminalReason: stringField(
      value,
      "terminal_reason",
    ) as GitHubGateTerminalEvent["terminalReason"],
    ...(typeof value.summary === "string" ? { summary: value.summary } : {}),
    ...(typeof value.failure_summary === "string"
      ? { failureSummary: value.failure_summary }
      : {}),
    completedAt: stringField(value, "completed_at"),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(value: Record<string, unknown>, key: string): string {
  if (typeof value[key] !== "string" || value[key].trim() === "") {
    throw new Error(`Review terminal event ${key} is invalid`);
  }
  return value[key];
}

function numberField(value: Record<string, unknown>, key: string): number {
  if (
    typeof value[key] !== "number" ||
    !Number.isSafeInteger(value[key]) ||
    value[key] < 0
  ) {
    throw new Error(`Review terminal event ${key} is invalid`);
  }
  return value[key];
}
