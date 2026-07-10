import assert from "node:assert/strict";

const baseUrl = (
  process.env.RUSTY_CREW_DEBUG_ADMIN_BASE_URL ?? "http://127.0.0.1:9348"
).replace(/\/$/, "");
const providerAlias =
  process.env.RUSTY_CREW_CHAT_CERT_PROVIDER_ALIAS ??
  "responses-proxy-cert-5389";
const suffix = Date.now().toString(36);
const profileId = `chat-cert-${suffix}`;
const recordSuffix = suffix.slice(-8);
let sessionId: string | undefined;

try {
  const health = await request("GET", "/v1/admin/healthz");
  assert.equal(health.status, 200);

  const created = await request("POST", "/v1/admin/control/profiles", {
    profileId,
    displayName: `Chat authority certification ${suffix}`,
    providerAlias,
    kind: "full",
    localToolProfileId: "full_agent",
    reason: "live Rust chat authority certification",
  });
  assert.equal(created.status, 200, created.text);
  sessionId = nestedString(created.json, [
    "data",
    "outcome",
    "result",
    "sessionId",
  ]);
  assert.ok(
    sessionId,
    "create-profile result must report the derived session id",
  );

  const streamAbort = new AbortController();
  const streamResponse = await fetch(
    `${baseUrl}/v1/chat/sessions/${encodeURIComponent(sessionId)}/stream`,
    { signal: streamAbort.signal },
  );
  assert.equal(streamResponse.status, 200);
  const streamedPromise = collectSseUntil(
    streamResponse,
    (events) =>
      events.some((event) => event.kind === "assistant_message_completed"),
    streamAbort,
    300_000,
  );

  const messageKey = `chat-cert-${suffix}`;
  const sent = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/messages`,
    {
      actor: { id: "cert-operator", kind: "human" },
      body: [
        "Use the git_status tool for /home/dev/rusty-crew and read_file on /home/dev/rusty-crew/README.md.",
        `Then answer with the exact marker CHAT_AUTHORITY_${suffix} and one sentence about what you checked.`,
      ].join("\n"),
      client_message_id: `message:${messageKey}`,
      reason: "live Rust chat authority certification",
    },
    { "Idempotency-Key": messageKey },
  );
  assert.equal(sent.status, 202, sent.text);
  const messageId = nestedString(sent.json, ["data", "message_id"]);
  assert.ok(messageId);

  const streamed = await streamedPromise;
  const terminalEvents = streamed.filter(
    (event) => event.kind === "assistant_message_completed",
  );
  assert.equal(
    terminalEvents.length,
    1,
    "live stream must have one terminal message",
  );
  assert.equal(terminalEvents[0]?.payload.status, "completed");
  const completedTools = streamed.filter(
    (event) =>
      event.kind === "tool_call_completed" && event.payload.is_error !== true,
  );
  assert.ok(
    completedTools.length >= 1,
    "live provider turn must complete a tool",
  );

  const afterToolCursor =
    streamed.find((event) => event.kind === "tool_call_started")?.event_id ??
    streamed[0]?.event_id;
  assert.ok(afterToolCursor);
  const replay = await readSseOnce(
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/stream?once=true&limit=500&cursor=${encodeURIComponent(afterToolCursor)}`,
  );
  const replayApi = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=500&cursor=${encodeURIComponent(afterToolCursor)}`,
  );
  assert.equal(replayApi.status, 200, replayApi.text);
  const replayApiEvents = nestedArray(replayApi.json, ["data", "items"]);
  assert.deepEqual(
    replay.map((event) => event.event_id),
    replayApiEvents.map((event) => String(event.event_id)),
    "SSE reconnect must replay the exact durable event suffix",
  );

  const tree = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/tree`,
  );
  assert.equal(tree.status, 200, tree.text);
  const defaultBranch = nestedArray(tree.json, ["data", "branches"])[0];
  const defaultBranchId = String(defaultBranch?.branch_id ?? "");
  assert.ok(defaultBranchId);

  const slotId = `slot:cert:${recordSuffix}`;
  const primaryVariantId = `variant:cert:${recordSuffix}:primary`;
  const manualSlot = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/slots`,
    {
      slot_id: slotId,
      primary_variant_id: primaryVariantId,
      message_id: `message:cert:${recordSuffix}:primary`,
      actor: { id: "cert-operator", kind: "human" },
      body: "certification primary",
    },
  );
  assert.equal(manualSlot.status, 201, manualSlot.text);
  const primaryBranchId = nestedString(manualSlot.json, [
    "data",
    "slot",
    "primary",
    "message",
    "branch_id",
  ]);
  assert.ok(primaryBranchId);

  const alternateId = `variant:cert:${recordSuffix}:alternate`;
  const alternate = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/slots/${encodeURIComponent(slotId)}/variants`,
    {
      variant_id: alternateId,
      message_id: `message:cert:${recordSuffix}:alternate`,
      actor: { id: profileId, kind: "agent" },
      body: "certification alternate",
    },
  );
  assert.equal(alternate.status, 201, alternate.text);
  assert.equal(
    nestedString(alternate.json, ["data", "variant", "message", "branch_id"]),
    primaryBranchId,
    "alternate lineage must be inherited inside the Rust transaction",
  );

  const selected = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/slots/${encodeURIComponent(slotId)}/active-variant`,
    { active_variant_id: alternateId, expected: { type: "primary" } },
  );
  assert.equal(selected.status, 200, selected.text);
  assert.equal(nestedString(selected.json, ["data", "status"]), "selected");

  const branchId = `branch:cert:${recordSuffix}`;
  const branch = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/branches`,
    {
      branch_id: branchId,
      parent_branch_id: defaultBranchId,
      parent_message_id: messageId,
      origin_message_id: messageId,
      head_message_id: messageId,
      label: "Certification branch",
    },
  );
  assert.equal(branch.status, 201, branch.text);
  const selectedBranch = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/branches/active`,
    {
      active_branch_id: branchId,
      expected: { type: "branch", branch_id: defaultBranchId },
    },
  );
  assert.equal(selectedBranch.status, 200, selectedBranch.text);

  const snapshotId = `snapshot:cert:${recordSuffix}`;
  const snapshot = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/snapshots`,
    {
      snapshot_id: snapshotId,
      branch_id: branchId,
      message_id: messageId,
      cursor: terminalEvents[0]?.event_id,
      label: "Certification snapshot",
      summary: "Live chat authority certification snapshot",
      source: "user",
    },
  );
  assert.equal(snapshot.status, 201, snapshot.text);
  const jump = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/jump?target_type=snapshot&snapshot_id=${encodeURIComponent(snapshotId)}`,
  );
  assert.equal(jump.status, 200, jump.text);
  assert.equal(nestedString(jump.json, ["data", "snapshot_id"]), snapshotId);

  const scopeId = `scope:cert:${recordSuffix}`;
  const scope = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/data-bank/scopes`,
    { scope_id: scopeId, label: "Certification files" },
  );
  assert.equal(scope.status, 201, scope.text);
  const attachmentId = `attachment:cert:${recordSuffix}`;
  const attachment = await request(
    "POST",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/attachments`,
    {
      attachment_id: attachmentId,
      filename: "certification.txt",
      mime_type: "text/plain",
      byte_size: 24,
      extracted_text: "chat authority certified",
      extracted_text_truncated: false,
      message_id: messageId,
      scope_id: scopeId,
    },
  );
  assert.equal(attachment.status, 201, attachment.text);
  const attachmentPage = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/attachments?message_id=${encodeURIComponent(messageId)}`,
  );
  assert.equal(attachmentPage.status, 200, attachmentPage.text);
  assert.equal(nestedNumber(attachmentPage.json, ["data", "total"]), 1);

  const removedAttachment = await request(
    "DELETE",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/attachments/${encodeURIComponent(attachmentId)}`,
  );
  assert.equal(removedAttachment.status, 200, removedAttachment.text);
  const removedScope = await request(
    "DELETE",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/data-bank/scopes/${encodeURIComponent(scopeId)}`,
  );
  assert.equal(removedScope.status, 200, removedScope.text);

  const finalEvents = await request(
    "GET",
    `/v1/chat/sessions/${encodeURIComponent(sessionId)}/events?limit=500&cursor=${encodeURIComponent(`${sessionId}:0`)}`,
  );
  assert.equal(finalEvents.status, 200, finalEvents.text);
  const finalItems = nestedArray(finalEvents.json, ["data", "items"]);
  assert.equal(
    finalItems.filter((event) => event.kind === "assistant_message_completed")
      .length,
    1,
    "one live request must persist one terminal assistant message",
  );

  console.log(
    JSON.stringify(
      {
        backend: baseUrl,
        profileId,
        sessionId,
        providerAlias,
        streamedEvents: streamed.length,
        completedTools: completedTools.map((event) => event.payload.tool_name),
        replayedEvents: replay.length,
        activeVariant: alternateId,
        activeBranch: branchId,
        snapshot: snapshotId,
        attachmentRemoved: attachmentId,
        scopeRemoved: scopeId,
      },
      null,
      2,
    ),
  );
} finally {
  if (sessionId !== undefined) {
    const deleted = await request(
      "POST",
      `/v1/admin/control/profiles/${encodeURIComponent(profileId)}/delete`,
      { confirmProfileId: profileId, reason: "live certification cleanup" },
    ).catch(() => undefined);
    if (deleted !== undefined && deleted.status >= 400) {
      console.error(
        `profile cleanup failed: HTTP ${deleted.status} ${deleted.text}`,
      );
    }
  }
}

interface ApiResponse {
  status: number;
  text: string;
  json: Record<string, unknown>;
}

interface ChatEvent {
  event_id: string;
  sequence_id: number;
  kind: string;
  payload: Record<string, unknown>;
}

async function request(
  method: string,
  path: string,
  body?: unknown,
  headers: Record<string, string> = {},
): Promise<ApiResponse> {
  const response = await fetch(`${baseUrl}${path}`, {
    method,
    headers: {
      ...(body === undefined ? {} : { "content-type": "application/json" }),
      ...headers,
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(300_000),
  });
  const text = await response.text();
  let json: Record<string, unknown> = {};
  if (text.trim() !== "") {
    json = JSON.parse(text) as Record<string, unknown>;
  }
  return { status: response.status, text, json };
}

async function readSseOnce(path: string): Promise<ChatEvent[]> {
  const response = await fetch(`${baseUrl}${path}`, {
    signal: AbortSignal.timeout(30_000),
  });
  assert.equal(response.status, 200);
  return collectSse(response);
}

async function collectSseUntil(
  response: Response,
  done: (events: ChatEvent[]) => boolean,
  controller: AbortController,
  timeoutMs: number,
): Promise<ChatEvent[]> {
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await collectSse(response, done);
  } finally {
    clearTimeout(timeout);
    controller.abort();
  }
}

async function collectSse(
  response: Response,
  done?: (events: ChatEvent[]) => boolean,
): Promise<ChatEvent[]> {
  assert.ok(response.body, "SSE response must have a body");
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  const events: ChatEvent[] = [];
  let buffer = "";
  while (true) {
    const next = await reader.read();
    if (next.done) break;
    buffer += decoder.decode(next.value, { stream: true });
    let boundary = buffer.indexOf("\n\n");
    while (boundary >= 0) {
      const frame = buffer.slice(0, boundary);
      buffer = buffer.slice(boundary + 2);
      const data = frame
        .split("\n")
        .filter((line) => line.startsWith("data:"))
        .map((line) => line.slice(5).trimStart())
        .join("\n");
      if (data !== "") events.push(JSON.parse(data) as ChatEvent);
      if (done?.(events)) {
        await reader.cancel();
        return events;
      }
      boundary = buffer.indexOf("\n\n");
    }
  }
  return events;
}

function nestedValue(value: unknown, path: readonly string[]): unknown {
  let current = value;
  for (const key of path) {
    if (
      typeof current !== "object" ||
      current === null ||
      Array.isArray(current)
    ) {
      return undefined;
    }
    current = (current as Record<string, unknown>)[key];
  }
  return current;
}

function nestedString(
  value: unknown,
  path: readonly string[],
): string | undefined {
  const result = nestedValue(value, path);
  return typeof result === "string" ? result : undefined;
}

function nestedNumber(
  value: unknown,
  path: readonly string[],
): number | undefined {
  const result = nestedValue(value, path);
  return typeof result === "number" ? result : undefined;
}

function nestedArray(
  value: unknown,
  path: readonly string[],
): Array<Record<string, unknown>> {
  const result = nestedValue(value, path);
  return Array.isArray(result)
    ? result.filter(
        (item): item is Record<string, unknown> =>
          typeof item === "object" && item !== null && !Array.isArray(item),
      )
    : [];
}
