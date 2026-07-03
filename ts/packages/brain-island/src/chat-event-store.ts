import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { appendFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import type { SessionId } from "@rusty-crew/contracts";
import type { ChatEvent } from "./rusty-view-chat-api.js";

export class ChatEventStore {
  readonly rootDir: string;
  private readonly latestSequences = new Map<string, number>();
  private readonly pendingLinesByPath = new Map<string, string[]>();
  private flushPromise: Promise<void> | undefined;

  constructor(rootDir: string) {
    this.rootDir = rootDir;
    mkdirSync(rootDir, { recursive: true });
  }

  append(event: ChatEvent): void {
    const path = this.pathForSession(event.session_id);
    const pending = this.pendingLinesByPath.get(path) ?? [];
    pending.push(`${JSON.stringify(event)}\n`);
    this.pendingLinesByPath.set(path, pending);
    this.latestSequences.set(event.session_id, event.sequence_id);
    void this.flush();
  }

  async flush(): Promise<void> {
    if (this.flushPromise !== undefined) return this.flushPromise;
    this.flushPromise = this.flushPending();
    try {
      await this.flushPromise;
    } finally {
      this.flushPromise = undefined;
    }
  }

  listAfterCursor(
    sessionId: SessionId,
    cursor: string | undefined,
    limit: number,
  ): readonly ChatEvent[] {
    if (limit <= 0) return [];
    const events = this.readSessionEvents(sessionId);
    if (cursor === undefined) {
      return events.slice(Math.max(0, events.length - limit));
    }
    const after = cursorSequence(cursor, sessionId);
    return events.filter((event) => event.sequence_id > after).slice(0, limit);
  }

  latestSequence(sessionId: SessionId): number | undefined {
    const cached = this.latestSequences.get(sessionId);
    if (cached !== undefined) return cached;
    const latest = this.readSessionEvents(sessionId).at(-1)?.sequence_id;
    if (latest !== undefined) this.latestSequences.set(sessionId, latest);
    return latest;
  }

  private readSessionEvents(sessionId: SessionId): ChatEvent[] {
    const path = this.pathForSession(sessionId);
    const persisted = existsSync(path) ? readFileSync(path, "utf8") : "";
    const pending = this.pendingLinesByPath.get(path)?.join("") ?? "";
    if (!persisted && !pending) return [];
    return `${persisted}${pending}`
      .split("\n")
      .filter((line) => line.trim().length > 0)
      .map((line) => parseChatEventLine(line))
      .filter(
        (event): event is ChatEvent =>
          event !== undefined && event.session_id === sessionId,
      )
      .sort((left, right) => left.sequence_id - right.sequence_id);
  }

  private pathForSession(sessionId: string): string {
    return join(this.rootDir, `${encodeURIComponent(sessionId)}.jsonl`);
  }

  private async flushPending(): Promise<void> {
    await mkdir(this.rootDir, { recursive: true });
    while (this.pendingLinesByPath.size > 0) {
      const batch = [...this.pendingLinesByPath.entries()];
      this.pendingLinesByPath.clear();
      await Promise.all(
        batch.map(([path, lines]) => appendFile(path, lines.join(""), "utf8")),
      );
    }
  }
}

function parseChatEventLine(line: string): ChatEvent | undefined {
  try {
    return JSON.parse(line) as ChatEvent;
  } catch {
    return undefined;
  }
}

function cursorSequence(cursor: string | undefined, sessionId: string): number {
  if (!cursor) return 0;
  const prefix = `${sessionId}:`;
  if (!cursor.startsWith(prefix)) return 0;
  const sequence = Number(cursor.slice(prefix.length));
  return Number.isSafeInteger(sequence) && sequence >= 0 ? sequence : 0;
}
