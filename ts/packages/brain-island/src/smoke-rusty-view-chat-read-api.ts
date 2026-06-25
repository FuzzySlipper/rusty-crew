import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { createServer as createHttpServer, type Server } from "node:http";
import { createServer as createTcpServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { AgentId, BrainEventEnvelope } from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type BrainWakeExecutor,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";
import { startRustyCrewServiceHost } from "./service-host.js";

const root = mkdtempSync(join(tmpdir(), "rusty-view-chat-read-api-"));
const port = await openPort();
const mcpPort = await openPort();
const token = "rusty-view-chat-token";
writeRuntimeConfig(root, mcpPort);
const mcpServer = await startMcpServer(mcpPort);
let host = await startHost();

try {
  const preflight = await options(
    "/v1/chat/sessions",
    "http://rusty-view.local",
  );
  assert.equal(preflight.status, 204);
  assert.equal(
    preflight.headers.get("access-control-allow-origin"),
    "http://rusty-view.local",
  );
  assert.ok(
    preflight.headers
      .get("access-control-allow-headers")
      ?.includes("authorization"),
  );

  const unauthorized = await get("/v1/chat/sessions", undefined, {
    origin: "http://rusty-view.local",
  });
  assert.equal(unauthorized.status, 401);
  assert.equal(
    unauthorized.headers.get("access-control-allow-origin"),
    "http://rusty-view.local",
  );

  const adminPreflight = await options(
    "/v1/admin/diagnostics",
    "http://rusty-view.local",
  );
  assert.equal(adminPreflight.status, 401);
  assert.equal(adminPreflight.headers.get("access-control-allow-origin"), null);

  const page = await get("/v1/chat/sessions", token, {
    origin: "http://rusty-view.local",
  });
  assert.equal(page.status, 200);
  assert.equal(
    page.headers.get("access-control-allow-origin"),
    "http://rusty-view.local",
  );
  assert.equal(page.body.ok, true);
  assert.equal(page.body.data.total, 2);
  assert.equal(page.body.data.items[0]?.session_id, "chat-session");
  assert.equal(typeof page.body.data.items[0]?.latest_cursor, "string");

  await host.bridge.routeAgentMessage(
    "human-operator" as AgentId,
    "chat-agent" as AgentId,
    "hello from Rusty View",
    "chat-smoke-1",
  );

  const opened = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(opened.status, 200);
  assert.equal(opened.body.data.session.session_id, "chat-session");
  assert.deepEqual(
    opened.body.data.events.map((event: { kind: string }) => event.kind),
    ["session_snapshot", "message_created"],
  );
  assert.equal(
    opened.body.data.events[1]?.payload.body,
    "hello from Rusty View",
  );
  assert.equal(opened.body.data.events[1]?.payload.role, "user");

  const missing = await get("/v1/chat/sessions/missing-session", token);
  assert.equal(missing.status, 404);
  assert.equal(missing.body.error.reason_code, "chat_session_not_found");

  const streamAbort = new AbortController();
  const streamResponse = await fetch(
    `http://127.0.0.1:${port}/v1/chat/sessions/chat-session/stream`,
    {
      headers: {
        authorization: `Bearer ${token}`,
        origin: "http://rusty-view.local",
      },
      signal: streamAbort.signal,
    },
  );
  assert.equal(streamResponse.status, 200);
  assert.equal(
    streamResponse.headers.get("access-control-allow-origin"),
    "http://rusty-view.local",
  );
  assert.ok(
    streamResponse.headers.get("content-type")?.includes("text/event-stream"),
  );
  const streamedEventsPromise = collectSseEventsUntil(
    streamResponse,
    (events) =>
      events.some((event) => event.kind === "assistant_text_delta") &&
      events.some((event) => event.kind === "tool_call_started") &&
      events.some((event) => event.kind === "tool_call_completed"),
    streamAbort,
  );

  let postSettled = false;
  const sentPromise = post(
    "/v1/chat/sessions/chat-session/messages",
    token,
    {
      actor: { id: "human-operator", kind: "human" },
      body: "please answer from the chat endpoint",
      client_message_id: "client-message-1",
    },
    { "Idempotency-Key": "chat-send-1" },
  ).finally(() => {
    postSettled = true;
  });
  const streamedEvents = await streamedEventsPromise;
  assert.equal(
    postSettled,
    false,
    "stream should receive assistant/tool progress before chat POST completes",
  );
  const sent = await sentPromise;
  assert.equal(sent.status, 202);
  assert.equal(sent.body.ok, true);
  assert.equal(sent.body.data.status, "accepted");
  assert.equal(sent.body.data.message_id, "client-message-1");
  assert.equal(sent.body.data.slot_id, "slot:client-message-1");
  assert.equal(
    sent.body.data.primary_variant_id,
    "variant:slot:client-message-1",
  );
  assert.equal(typeof sent.body.data.wake_id, "string");

  const initialTree = await get("/v1/chat/sessions/chat-session/tree", token);
  assert.equal(initialTree.status, 200);
  const defaultBranch = initialTree.body.data.branches.find(
    (branch: { label?: string }) => branch.label === "Default",
  );
  assert.ok(defaultBranch, "chat send should create a default branch");
  assert.equal(defaultBranch.head_message_id, "client-message-1");
  assert.equal(
    initialTree.body.data.branch_state.active_branch_id,
    defaultBranch.branch_id,
  );

  const messageJump = await get(
    "/v1/chat/sessions/chat-session/jump?target_type=message&message_id=client-message-1",
    token,
  );
  assert.equal(messageJump.status, 200);
  assert.equal(messageJump.body.data.branch_id, defaultBranch.branch_id);

  assert.ok(
    streamedEvents.some((event) => event.kind === "message_created"),
    "active stream should receive the submitted message event",
  );
  assert.ok(
    streamedEvents.some((event) => event.kind === "tool_call_completed"),
    "active stream should receive tool completion while wake is live",
  );

  const postTurnPage = await get("/v1/chat/sessions", token);
  assert.equal(postTurnPage.status, 200);
  const postTurnSession = postTurnPage.body.data.items.find(
    (item: { session_id: string }) => item.session_id === "chat-session",
  );
  assert.ok(postTurnSession, "chat-session should be listed after chat turn");
  assert.notEqual(
    postTurnSession.latest_cursor,
    "chat-session:0",
    "session latest_cursor should advance with chat events",
  );
  assert.ok(
    postTurnSession.message_count >= 2,
    "session message_count should include user and assistant chat messages",
  );

  const postTurnOpen = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(postTurnOpen.status, 200);
  assert.equal(
    postTurnOpen.body.data.session.latest_cursor,
    postTurnSession.latest_cursor,
  );
  assert.ok(postTurnOpen.body.data.session.message_count >= 2);
  assert.ok(
    postTurnOpen.body.data.message_slots.some(
      (slot: { slot_id: string; alternates: unknown[] }) =>
        slot.slot_id === sent.body.data.slot_id && slot.alternates.length === 0,
    ),
    "open session should include primary slots without lazy alternates",
  );

  const slots = await get(
    "/v1/chat/sessions/chat-session/slots?include_alternates=true",
    token,
  );
  assert.equal(slots.status, 200);
  const sentSlot = slots.body.data.items.find(
    (slot: { slot_id: string }) => slot.slot_id === sent.body.data.slot_id,
  );
  assert.ok(sentSlot, "sent message slot should be queryable");
  assert.equal(sentSlot.primary.variant_id, sent.body.data.primary_variant_id);

  const createdSlot = await post(
    "/v1/chat/sessions/chat-session/slots",
    token,
    {
      slot_id: "slot:manual",
      primary_variant_id: "variant:manual:primary",
      message_id: "message:manual:primary",
      actor: { id: "human-operator", kind: "human" },
      body: "manual primary",
    },
  );
  assert.equal(createdSlot.status, 201);
  assert.equal(createdSlot.body.data.status, "created");
  assert.equal(createdSlot.body.data.slot.slot_id, "slot:manual");
  assert.equal(
    createdSlot.body.data.slot.primary.variant_id,
    "variant:manual:primary",
  );

  const firstAlternate = await post(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/variants",
    token,
    {
      variant_id: "variant:manual:alt1",
      message_id: "message:manual:alt1",
      actor: { id: "chat-agent", kind: "agent" },
      body: "alternate one",
    },
  );
  assert.equal(firstAlternate.status, 201);
  assert.equal(firstAlternate.body.data.variant.source, "alternate");
  assert.equal(firstAlternate.body.data.variant.ordinal, 1);

  const secondAlternate = await post(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/variants",
    token,
    {
      variant_id: "variant:manual:alt2",
      message_id: "message:manual:alt2",
      actor: { id: "chat-agent", kind: "agent" },
      body: "alternate two",
    },
  );
  assert.equal(secondAlternate.status, 201);
  assert.equal(secondAlternate.body.data.variant.ordinal, 2);

  const variants = await get(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/variants",
    token,
  );
  assert.equal(variants.status, 200);
  assert.deepEqual(
    variants.body.data.items.map(
      (variant: { variant_id: string }) => variant.variant_id,
    ),
    ["variant:manual:primary", "variant:manual:alt1", "variant:manual:alt2"],
  );

  const selected = await post(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/active-variant",
    token,
    {
      active_variant_id: "variant:manual:alt1",
      expected: { type: "primary" },
    },
  );
  assert.equal(selected.status, 200);
  assert.equal(selected.body.data.status, "selected");
  assert.equal(
    selected.body.data.slot.active_variant_id,
    "variant:manual:alt1",
  );

  const conflict = await post(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/active-variant",
    token,
    {
      active_variant_id: "variant:manual:alt2",
      expected: { type: "primary" },
    },
  );
  assert.equal(conflict.status, 409);
  assert.equal(conflict.body.data.status, "conflict");
  assert.equal(conflict.body.data.conflict.actual, "variant:manual:alt1");

  const reordered = await post(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/variants/reorder",
    token,
    { ordered_variant_ids: ["variant:manual:alt2", "variant:manual:alt1"] },
  );
  assert.equal(reordered.status, 200);
  assert.deepEqual(
    reordered.body.data.variants.map(
      (variant: { variant_id: string }) => variant.variant_id,
    ),
    ["variant:manual:primary", "variant:manual:alt2", "variant:manual:alt1"],
  );

  const deleted = await del(
    "/v1/chat/sessions/chat-session/slots/slot%3Amanual/variants/variant%3Amanual%3Aalt1",
    token,
  );
  assert.equal(deleted.status, 200);
  assert.equal(deleted.body.data.status, "deleted");
  assert.equal(deleted.body.data.slot.active_variant_id, null);

  const lazyOpen = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(lazyOpen.status, 200);
  const lazyManualSlot = lazyOpen.body.data.message_slots.find(
    (slot: { slot_id: string }) => slot.slot_id === "slot:manual",
  );
  assert.ok(lazyManualSlot, "manual slot should hydrate on open");
  assert.deepEqual(
    lazyManualSlot.alternates,
    [],
    "open session should lazy-load alternates by default",
  );

  const eagerOpen = await get(
    "/v1/chat/sessions/chat-session?include_alternates=true",
    token,
  );
  assert.equal(eagerOpen.status, 200);
  const eagerManualSlot = eagerOpen.body.data.message_slots.find(
    (slot: { slot_id: string }) => slot.slot_id === "slot:manual",
  );
  assert.equal(eagerManualSlot.alternates.length, 1);
  assert.equal(eagerManualSlot.alternates[0].variant_id, "variant:manual:alt2");

  const createdBranch = await post(
    "/v1/chat/sessions/chat-session/branches",
    token,
    {
      branch_id: "branch:manual",
      parent_branch_id: defaultBranch.branch_id,
      parent_message_id: "client-message-1",
      origin_message_id: "client-message-1",
      head_message_id: "client-message-1",
      label: "Manual branch",
    },
  );
  assert.equal(createdBranch.status, 201);
  assert.equal(createdBranch.body.data.branch.branch_id, "branch:manual");

  const selectedBranch = await post(
    "/v1/chat/sessions/chat-session/branches/active",
    token,
    {
      active_branch_id: "branch:manual",
      expected: { type: "branch", branch_id: defaultBranch.branch_id },
    },
  );
  assert.equal(selectedBranch.status, 200);
  assert.equal(selectedBranch.body.data.status, "selected");
  assert.equal(
    selectedBranch.body.data.state.active_branch_id,
    "branch:manual",
  );

  const branchConflict = await post(
    "/v1/chat/sessions/chat-session/branches/active",
    token,
    {
      active_branch_id: defaultBranch.branch_id,
      expected: { type: "none" },
    },
  );
  assert.equal(branchConflict.status, 409);
  assert.equal(branchConflict.body.data.status, "conflict");
  assert.equal(branchConflict.body.data.conflict.actual, "branch:manual");

  const headUpdate = await post(
    "/v1/chat/sessions/chat-session/branches/branch%3Amanual/head",
    token,
    {
      head_message_id: "client-message-1",
      expected: { type: "message", message_id: "client-message-1" },
    },
  );
  assert.equal(headUpdate.status, 200);
  assert.equal(headUpdate.body.data.status, "updated");

  const createdSnapshot = await post(
    "/v1/chat/sessions/chat-session/snapshots",
    token,
    {
      snapshot_id: "snapshot:manual",
      branch_id: "branch:manual",
      message_id: "client-message-1",
      cursor: "chat-session:1",
      label: "Manual snapshot",
      summary: "Snapshot summary",
      source: "user",
    },
  );
  assert.equal(createdSnapshot.status, 201);
  assert.equal(
    createdSnapshot.body.data.snapshot.snapshot_id,
    "snapshot:manual",
  );

  const snapshotJump = await get(
    "/v1/chat/sessions/chat-session/jump?target_type=snapshot&snapshot_id=snapshot%3Amanual",
    token,
  );
  assert.equal(snapshotJump.status, 200);
  assert.equal(snapshotJump.body.data.cursor, "chat-session:1");

  const branchJump = await get(
    "/v1/chat/sessions/chat-session/jump?target_type=branch&branch_id=branch%3Amanual",
    token,
  );
  assert.equal(branchJump.status, 200);
  assert.equal(branchJump.body.data.message_id, "client-message-1");

  const treeAfterBranch = await get(
    "/v1/chat/sessions/chat-session/tree",
    token,
  );
  assert.equal(treeAfterBranch.status, 200);
  assert.ok(
    treeAfterBranch.body.data.snapshots.some(
      (snapshot: { snapshot_id: string }) =>
        snapshot.snapshot_id === "snapshot:manual",
    ),
  );

  const slotMutationKinds = (
    await get(
      "/v1/chat/sessions/chat-session/events?cursor=chat-session:0",
      token,
    )
  ).body.data.items
    .map((event: { kind: string }) => event.kind)
    .filter((kind: string) => kind.startsWith("message_"));
  for (const kind of [
    "message_slot_created",
    "message_variant_created",
    "message_active_variant_selected",
    "message_variants_reordered",
    "message_variant_deleted",
  ]) {
    assert.ok(slotMutationKinds.includes(kind), `missing ${kind} event`);
  }

  const afterMutationPage = await get("/v1/chat/sessions", token);
  const afterMutationSession = afterMutationPage.body.data.items.find(
    (item: { session_id: string }) => item.session_id === "chat-session",
  );
  assert.ok(afterMutationSession, "chat-session should still be listed");
  const afterLatest = await getSseOnce(
    `/v1/chat/sessions/chat-session/stream?once=true&cursor=${encodeURIComponent(
      afterMutationSession.latest_cursor,
    )}`,
    token,
  );
  assert.equal(
    afterLatest.length,
    0,
    "stream replay from latest_cursor should not replay historical turn events",
  );

  const eventsAfterSnapshot = await get(
    `/v1/chat/sessions/chat-session/events?cursor=${encodeURIComponent(
      "chat-session:0",
    )}`,
    token,
  );
  assert.equal(eventsAfterSnapshot.status, 200);
  assert.ok(eventsAfterSnapshot.body.data.items.length >= 4);
  assert.equal(eventsAfterSnapshot.body.data.items[0]?.kind, "message_created");

  const replayCursor = streamedEvents[1]?.event_id;
  assert.equal(typeof replayCursor, "string");
  const replay = await getSseOnce(
    `/v1/chat/sessions/chat-session/stream?once=true&cursor=${encodeURIComponent(
      replayCursor,
    )}`,
    token,
  );
  assert.ok(
    replay.every((event) => event.sequence_id > streamedEvents[1].sequence_id),
    "cursor replay should only return missed events",
  );

  const duplicate = await post(
    "/v1/chat/sessions/chat-session/messages",
    token,
    {
      actor: { id: "human-operator", kind: "human" },
      body: "this duplicate should not dispatch",
      client_message_id: "client-message-1",
    },
    { "Idempotency-Key": "chat-send-1" },
  );
  assert.equal(duplicate.status, 202);
  assert.equal(duplicate.body.data.status, "duplicate");
  assert.equal(duplicate.body.data.wake_id, sent.body.data.wake_id);

  const empty = await post("/v1/chat/sessions/chat-session/messages", token, {
    actor: { id: "human-operator", kind: "human" },
    body: " ",
  });
  assert.equal(empty.status, 400);
  assert.equal(empty.body.error.reason_code, "empty_chat_message");

  const missingSend = await post("/v1/chat/sessions/missing/messages", token, {
    actor: { id: "human-operator", kind: "human" },
    body: "hello",
  });
  assert.equal(missingSend.status, 404);

  const registry = await get("/v1/chat/commands", token);
  assert.equal(registry.status, 200);
  assert.deepEqual(
    registry.body.data.commands.map(
      (command: { name: string }) => command.name,
    ),
    ["help", "status", "session", "new", "reload-mcp"],
  );

  const statusCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/status",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(statusCommand.status, 200);
  assert.equal(statusCommand.body.data.status, "completed");
  assert.equal(statusCommand.body.data.command_name, "status");

  const unknownCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/definitely-not-real",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(unknownCommand.status, 409);
  assert.equal(unknownCommand.body.data.status, "rejected");
  assert.equal(unknownCommand.body.data.reason_code, "unknown_command");

  const reloadCommand = await post(
    "/v1/chat/sessions/mcp-session/commands",
    token,
    {
      command: "/reload-mcp",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(reloadCommand.status, 200);
  assert.equal(reloadCommand.body.data.command_name, "reload-mcp");
  assert.equal(reloadCommand.body.data.status, "completed");

  const newCommand = await post(
    "/v1/chat/sessions/chat-session/commands",
    token,
    {
      command: "/new fresh start",
      actor: { id: "human-operator", kind: "human" },
    },
  );
  assert.equal(newCommand.status, 200);
  assert.equal(newCommand.body.data.command_name, "new");
  assert.equal(newCommand.body.data.status, "completed");
  assert.equal(newCommand.body.data.old_session_id, "chat-session");
  assert.equal(typeof newCommand.body.data.new_session_id, "string");

  await host.stop();
  host = await startHost();

  const restarted = await get("/v1/chat/sessions/chat-session", token);
  assert.equal(restarted.status, 200);
  assert.equal(restarted.body.data.events[0]?.kind, "session_snapshot");
  assert.equal(restarted.body.data.session.session_id, "chat-session");

  await host.stop();
  host = await startHost({ RUSTY_CREW_ADMIN_AUTH_MODE: "none" });
  const noAuth = await get("/v1/chat/commands", undefined, {
    origin: "http://rusty-view.local",
  });
  assert.equal(noAuth.status, 200);
  assert.equal(noAuth.body.ok, true);
  assert.equal(
    noAuth.headers.get("access-control-allow-origin"),
    "http://rusty-view.local",
  );

  console.log(
    JSON.stringify(
      {
        sessions: page.body.data.total,
        openedEvents: opened.body.data.events.length,
        sendStatus: sent.body.data.status,
        duplicateStatus: duplicate.body.data.status,
        streamedEvents: streamedEvents.map((event) => event.kind),
        restartEvent: restarted.body.data.events[0]?.kind,
      },
      null,
      2,
    ),
  );
} finally {
  await host.stop().catch(() => undefined);
  await closeServer(mcpServer).catch(() => undefined);
  rmSync(root, { recursive: true, force: true });
}

async function startHost(extraEnv: Record<string, string> = {}) {
  const bridge = withLiveWakeEventsBridge(await loadNativeBridge());
  return startRustyCrewServiceHost({
    env: {
      RUSTY_CREW_DATA_DIR: root,
      RUSTY_CREW_ADMIN_HOST: "127.0.0.1",
      RUSTY_CREW_ADMIN_ALLOW_LAN: "false",
      RUSTY_CREW_ADMIN_PORT: String(port),
      RUSTY_CREW_ADMIN_TOKEN: token,
      RUSTY_CREW_SCHEDULER_TICK_INTERVAL_MS: "0",
      RUSTY_CREW_WAKE_DISPATCH_INTERVAL_MS: "0",
      ...extraEnv,
    },
    bridge,
  });
}

async function get(
  path: string,
  bearer?: string,
  extraHeaders: Record<string, string> = {},
) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
      ...extraHeaders,
    },
  });
  return {
    status: response.status,
    headers: response.headers,
    body: (await response.json()) as any,
  };
}

async function options(path: string, origin: string) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "OPTIONS",
    headers: {
      origin,
      "access-control-request-method": "GET",
      "access-control-request-headers": "authorization,content-type",
    },
  });
  return {
    status: response.status,
    headers: response.headers,
  };
}

async function post(
  path: string,
  bearer: string | undefined,
  body: unknown,
  extraHeaders: Record<string, string> = {},
) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "POST",
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
      "content-type": "application/json",
      ...extraHeaders,
    },
    body: JSON.stringify(body),
  });
  return {
    status: response.status,
    headers: response.headers,
    body: (await response.json()) as any,
  };
}

async function del(path: string, bearer: string | undefined) {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    method: "DELETE",
    headers: {
      ...(bearer ? { authorization: `Bearer ${bearer}` } : {}),
    },
  });
  return {
    status: response.status,
    headers: response.headers,
    body: (await response.json()) as any,
  };
}

async function getSseOnce(path: string, bearer: string): Promise<SseEvent[]> {
  const response = await fetch(`http://127.0.0.1:${port}${path}`, {
    headers: { authorization: `Bearer ${bearer}` },
  });
  assert.equal(response.status, 200);
  assert.ok(
    response.headers.get("content-type")?.includes("text/event-stream"),
  );
  const text = await response.text();
  return parseSseEvents(text);
}

async function collectSseEvents(
  response: Response,
  count: number,
  controller: AbortController,
): Promise<SseEvent[]> {
  return collectSseEventsUntil(
    response,
    (events) => events.length >= count,
    controller,
  );
}

async function collectSseEventsUntil(
  response: Response,
  done: (events: SseEvent[]) => boolean,
  controller: AbortController,
): Promise<SseEvent[]> {
  assert.ok(response.body, "SSE response should have a body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  const deadline = Date.now() + 5_000;
  try {
    while (!done(parseSseEvents(text)) && Date.now() < deadline) {
      const remaining = Math.max(deadline - Date.now(), 1);
      const read = await Promise.race([
        reader.read(),
        new Promise<ReadableStreamReadResult<Uint8Array>>((resolve) =>
          setTimeout(
            () => resolve({ done: true, value: undefined }),
            remaining,
          ),
        ),
      ]);
      if (read.done) break;
      text += decoder.decode(read.value, { stream: true });
    }
  } finally {
    controller.abort();
    await reader.cancel().catch(() => undefined);
  }
  const events = parseSseEvents(text);
  assert.ok(
    done(events),
    `SSE stream did not reach expected condition; received ${events
      .map((event) => event.kind)
      .join(", ")}`,
  );
  return events;
}

function parseSseEvents(text: string): SseEvent[] {
  return text
    .split("\n\n")
    .map((block) => block.trim())
    .filter((block) => block.includes("data: "))
    .map((block) => {
      const data = block
        .split("\n")
        .find((line) => line.startsWith("data: "))
        ?.slice("data: ".length);
      assert.ok(data, "SSE event should include data");
      return JSON.parse(data) as SseEvent;
    });
}

interface SseEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
}

function withLiveWakeEventsBridge(
  bridge: NativeBridgeModule,
): NativeBridgeModule {
  return {
    ...bridge,
    registerBrainRuntime: async (registration, executor) => {
      const wrappedExecutor: BrainWakeExecutor = {
        wake: async (request, buffers) => {
          const liveEvents = submitLiveWakeEvents(bridge, [
            {
              wakeId: request.wakeId,
              sessionId: request.sessionId,
              event: { type: "text_delta", text: "live streaming delta" },
            },
            {
              wakeId: request.wakeId,
              sessionId: request.sessionId,
              event: {
                type: "tool_call_started",
                toolName: "rusty_view_live_tool",
              },
            },
            {
              wakeId: request.wakeId,
              sessionId: request.sessionId,
              event: {
                type: "tool_call_finished",
                toolName: "rusty_view_live_tool",
                isError: false,
              },
            },
          ]);
          const [result] = await Promise.all([
            executor.wake(request, buffers),
            delay(200),
            liveEvents,
          ]);
          return result;
        },
      };
      return bridge.registerBrainRuntime(registration, wrappedExecutor);
    },
  };
}

async function submitLiveWakeEvents(
  bridge: NativeBridgeModule,
  events: BrainEventEnvelope[],
): Promise<void> {
  await delay(25);
  for (const event of events) {
    await bridge.submitBrainEvent(event);
    await delay(10);
  }
}

async function delay(ms: number): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

function writeRuntimeConfig(dataRoot: string, mcpServerPort: number): void {
  const configDir = join(dataRoot, "config");
  const profilesDir = join(configDir, "profiles");
  const skillsDir = join(configDir, "skills");
  mkdirSync(profilesDir, { recursive: true });
  mkdirSync(skillsDir, { recursive: true });
  writeFileSync(
    join(configDir, "service.json"),
    JSON.stringify(
      {
        profilesDir,
        skillsDir,
        brains: [{ profileId: "chat-profile" }],
        sessions: [
          {
            sessionId: "chat-session",
            agentId: "chat-agent",
            profileId: "chat-profile",
            kind: "full",
          },
          {
            sessionId: "mcp-session",
            agentId: "mcp-agent",
            profileId: "chat-profile",
            kind: "full",
          },
        ],
        mcpBindings: [
          {
            bindingId: "mcp-binding",
            adapterId: "mcp-ts-main",
            agentId: "mcp-agent",
            sessionId: "mcp-session",
            profileId: "chat-profile",
            serverNames: ["mcp-smoke"],
            endpointRef: `http://127.0.0.1:${mcpServerPort}/mcp`,
            transport: "streamable_http",
            toolProfileKey: "chat-profile",
            status: "active",
            diagnostics: {},
          },
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    join(profilesDir, "chat-profile.json"),
    JSON.stringify(
      {
        profileId: "chat-profile",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        prompt: {
          system: "Chat profile system prompt.",
          instructions: ["Answer concisely."],
        },
        toolPolicy: {
          requestedTools: [],
        },
      },
      null,
      2,
    ),
  );
}

function startMcpServer(portToListen: number): Promise<Server> {
  const server = createHttpServer(async (request, response) => {
    if (request.method !== "POST" || request.url !== "/mcp") {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { "content-type": "application/json" });
    response.end(
      JSON.stringify({
        jsonrpc: "2.0",
        id: 1,
        result: {
          tools: [
            {
              name: "smoke_tool",
              description: "Smoke MCP tool.",
              inputSchema: { type: "object", properties: {} },
            },
          ],
        },
      }),
    );
  });
  return new Promise((resolveListen, rejectListen) => {
    server.once("error", rejectListen);
    server.listen(portToListen, "127.0.0.1", () => resolveListen(server));
  });
}

function openPort(): Promise<number> {
  return new Promise((resolveOpenPort, rejectOpenPort) => {
    const server = createTcpServer();
    server.once("error", rejectOpenPort);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close();
        rejectOpenPort(new Error("failed to discover open TCP port"));
        return;
      }
      const open = address.port;
      server.close((error) => {
        if (error) rejectOpenPort(error);
        else resolveOpenPort(open);
      });
    });
  });
}

function closeServer(server: Server): Promise<void> {
  return new Promise((resolveClose, rejectClose) => {
    server.close((error) => (error ? rejectClose(error) : resolveClose()));
  });
}
