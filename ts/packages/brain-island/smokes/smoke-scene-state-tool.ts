import assert from "node:assert/strict";
import type { NativeSimpleKvRecord } from "@rusty-crew/native-bridge";
import {
  getSceneStateTool,
  updateSceneStateTool,
} from "../src/scene-state-tool.js";

async function runSmoke(): Promise<void> {
  const bridge = new FakeSimpleKvBridge();
  const context = {
    client: bridge,
    session: { sessionId: "rp-session-1" },
    now: () => "2026-07-03T21:00:00Z",
  };

  const empty = await getSceneStateTool(context).execute("get-empty", {});
  assert.equal(empty.details.ok, true);
  assert.deepEqual(empty.details.state, {
    sessionId: "rp-session-1",
    charactersPresent: [],
    activeThreads: [],
  });

  const updated = await updateSceneStateTool(context).execute("update", {
    location: "Moonlit Garden",
    charactersPresent: ["elara", "katheryn"],
    activeThreads: ["missing locket"],
    notes: "The air is warm after rain.",
  });
  assert.equal(updated.details.ok, true);
  assert.equal(updated.details.state?.location, "Moonlit Garden");
  assert.deepEqual(updated.details.state?.charactersPresent, [
    "elara",
    "katheryn",
  ]);
  assert.equal(bridge.records.get(key("rp-session-1"))?.revision, 1);

  const patched = await updateSceneStateTool(context).execute("patch", {
    notes: null,
    activeThreads: ["missing locket", "garden promise"],
  });
  assert.equal(patched.details.ok, true);
  assert.equal(patched.details.state?.notes, undefined);
  assert.deepEqual(patched.details.state?.activeThreads, [
    "missing locket",
    "garden promise",
  ]);
  assert.equal(bridge.records.get(key("rp-session-1"))?.revision, 2);

  const reread = await getSceneStateTool(context).execute("get", {});
  assert.equal(reread.details.ok, true);
  assert.equal(reread.details.state?.location, "Moonlit Garden");

  const denied = await getSceneStateTool({ client: bridge }).execute(
    "missing-session",
    {},
  );
  assert.equal(denied.details.ok, false);
  assert.equal(denied.details.reasonCode, "session_id_missing");

  console.log(
    JSON.stringify(
      {
        location: reread.details.state?.location,
        activeThreads: reread.details.state?.activeThreads,
        revision: reread.details.revision,
        denied: denied.details.reasonCode,
      },
      null,
      2,
    ),
  );
}

class FakeSimpleKvBridge {
  readonly records = new Map<string, NativeSimpleKvRecord>();

  async readRoleplaySceneState(input: {
    session_id: string;
    record_value_json?: string;
    record_updated_at?: string;
    revision?: number;
  }): Promise<{
    state: {
      sessionId: string;
      location?: string;
      charactersPresent: string[];
      activeThreads: string[];
      notes?: string;
      updatedAt?: string;
    };
    revision?: number;
  }> {
    const parsed =
      input.record_value_json === undefined
        ? undefined
        : safeJson(input.record_value_json);
    return {
      state: compactState({
        sessionId: input.session_id,
        location: normalizedString(parsed?.location),
        charactersPresent: normalizedStringArray(parsed?.charactersPresent),
        activeThreads: normalizedStringArray(parsed?.activeThreads),
        notes: normalizedString(parsed?.notes),
        updatedAt:
          normalizedString(parsed?.updatedAt) ??
          normalizedString(input.record_updated_at),
      }),
      revision: input.revision,
    };
  }

  async planRoleplaySceneStateUpdate(input: {
    session_id: string;
    current?: {
      sessionId: string;
      location?: string;
      charactersPresent: string[];
      activeThreads: string[];
      notes?: string;
      updatedAt?: string;
    };
    now: string;
    body: {
      location?: string | null;
      charactersPresent?: string[];
      activeThreads?: string[];
      notes?: string | null;
    };
  }): Promise<{
    state: {
      sessionId: string;
      location?: string;
      charactersPresent: string[];
      activeThreads: string[];
      notes?: string;
      updatedAt?: string;
    };
    value_json: string;
    now: string;
  }> {
    const state = compactState({
      sessionId: input.session_id,
      location: input.current?.location,
      charactersPresent: input.current?.charactersPresent ?? [],
      activeThreads: input.current?.activeThreads ?? [],
      notes: input.current?.notes,
      updatedAt: input.now,
    });
    if ("location" in input.body) {
      state.location = normalizedString(input.body.location);
    }
    if (input.body.charactersPresent !== undefined) {
      state.charactersPresent = normalizedStringArray(
        input.body.charactersPresent,
      );
    }
    if (input.body.activeThreads !== undefined) {
      state.activeThreads = normalizedStringArray(input.body.activeThreads);
    }
    if ("notes" in input.body) {
      state.notes = normalizedString(input.body.notes);
    }
    return {
      state,
      value_json: JSON.stringify(state),
      now: input.now,
    };
  }

  async listSimpleKv(query: {
    scopeType: string;
    scopeId: string;
    keyPrefix?: string;
  }): Promise<NativeSimpleKvRecord[]> {
    return [...this.records.values()].filter(
      (record) =>
        record.scopeType === query.scopeType &&
        record.scopeId === query.scopeId &&
        (!query.keyPrefix || record.key.startsWith(query.keyPrefix)),
    );
  }

  async putSimpleKv(write: {
    scopeType: string;
    scopeId: string;
    key: string;
    valueJson: string;
    now: string;
    expiresAt?: string;
  }): Promise<NativeSimpleKvRecord> {
    const recordKey = key(write.scopeId);
    const existing = this.records.get(recordKey);
    const record: NativeSimpleKvRecord = {
      scopeType: write.scopeType,
      scopeId: write.scopeId,
      key: write.key,
      valueJson: write.valueJson,
      revision: (existing?.revision ?? 0) + 1,
      createdAt: existing?.createdAt ?? write.now,
      updatedAt: write.now,
      expiresAt: write.expiresAt,
    };
    this.records.set(recordKey, record);
    return record;
  }
}

function safeJson(value: string): Record<string, unknown> | undefined {
  try {
    const parsed = JSON.parse(value) as unknown;
    return typeof parsed === "object" &&
      parsed !== null &&
      !Array.isArray(parsed)
      ? (parsed as Record<string, unknown>)
      : undefined;
  } catch {
    return undefined;
  }
}

function normalizedString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim()
    : undefined;
}

function normalizedStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  return value
    .filter((item): item is string => typeof item === "string")
    .map((item) => item.trim())
    .filter((item) => item.length > 0)
    .filter((item) => {
      if (seen.has(item)) return false;
      seen.add(item);
      return true;
    });
}

function compactState(state: {
  sessionId: string;
  location?: string;
  charactersPresent: string[];
  activeThreads: string[];
  notes?: string;
  updatedAt?: string;
}): {
  sessionId: string;
  location?: string;
  charactersPresent: string[];
  activeThreads: string[];
  notes?: string;
  updatedAt?: string;
} {
  return {
    sessionId: state.sessionId,
    ...(state.location === undefined ? {} : { location: state.location }),
    charactersPresent: state.charactersPresent,
    activeThreads: state.activeThreads,
    ...(state.notes === undefined ? {} : { notes: state.notes }),
    ...(state.updatedAt === undefined ? {} : { updatedAt: state.updatedAt }),
  };
}

function key(sessionId: string): string {
  return `roleplay_scene_state:${sessionId}:current`;
}

await runSmoke();
