import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CodexAppServerDriver,
  CodexProtocolCodec,
  type CodexControllerAuthority,
  type CodexJsonRpcTransport,
  type CodexProtocolFault,
  type CodexTransportHandlers,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "../src/index.js";

class FakeTransport implements CodexJsonRpcTransport {
  handlers?: CodexTransportHandlers;
  readonly sent: Array<Record<string, unknown>> = [];
  readonly responders = new Map<
    string,
    (request: Record<string, unknown>) => unknown
  >();
  opened = false;
  closed = false;

  setHandlers(handlers: CodexTransportHandlers): void {
    this.handlers = handlers;
  }

  async open(): Promise<void> {
    this.opened = true;
  }

  async send(message: string): Promise<void> {
    const parsed = JSON.parse(message) as Record<string, unknown>;
    this.sent.push(parsed);
    const method =
      typeof parsed.method === "string" ? parsed.method : undefined;
    if (method === undefined) return;
    const responder = this.responders.get(method);
    if (responder === undefined) return;
    queueMicrotask(() =>
      this.emit({ id: parsed.id, result: responder(parsed) }),
    );
  }

  async close(): Promise<void> {
    this.closed = true;
  }

  emit(message: unknown): void {
    this.handlers?.onMessage(JSON.stringify(message));
  }

  disconnect(reason = "test disconnect"): void {
    this.handlers?.onClose(reason);
  }
}

class FakeAuthority implements CodexControllerAuthority {
  accepted = true;
  leased = true;
  readonly events: NeutralExternalRuntimeEvent[] = [];
  readonly faults: CodexProtocolFault[] = [];
  readonly serverRequests: string[] = [];
  readonly disconnects: Array<{
    readonly reason: string;
    readonly pendingClientRequestIds: readonly (string | number)[];
    readonly pendingServerRequestIds: readonly (string | number)[];
  }> = [];
  resolution: ServerRequestResolution = {
    type: "error",
    code: -32000,
    message: "not configured",
  };
  readonly resolutions = new Map<string, ServerRequestResolution>();
  resolver?: (
    context: Parameters<CodexControllerAuthority["resolveServerRequest"]>[0],
  ) => Promise<ServerRequestResolution>;

  async authorizeHandshake(): Promise<{ accepted: boolean }> {
    return { accepted: this.accepted };
  }

  hasControllerLease(): boolean {
    return this.leased;
  }

  onEvent(event: NeutralExternalRuntimeEvent): void {
    this.events.push(event);
  }

  async resolveServerRequest(
    context: Parameters<CodexControllerAuthority["resolveServerRequest"]>[0],
  ): Promise<ServerRequestResolution> {
    this.serverRequests.push(context.request.method);
    if (this.resolver !== undefined) return this.resolver(context);
    return this.resolutions.get(context.request.method) ?? this.resolution;
  }

  onProtocolFault(fault: CodexProtocolFault): void {
    this.faults.push(fault);
  }

  onDisconnected(details: (typeof this.disconnects)[number]): void {
    this.disconnects.push(details);
  }
}

function configureInitialize(transport: FakeTransport): void {
  transport.responders.set("initialize", () => ({
    userAgent: "codex_cli_rs/0.144.1",
    codexHome: "/tmp/codex-home",
    platformFamily: "unix",
    platformOs: "linux",
  }));
}

async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 10));
}

test("driver authorizes exact handshake before exposing typed requests", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  transport.responders.set("thread/list", () => ({
    data: [],
    nextCursor: null,
    backwardsCursor: null,
  }));
  transport.responders.set("collaborationMode/list", () => ({
    data: [
      {
        name: "Plan",
        mode: "plan",
        model: null,
        reasoning_effort: "medium",
      },
    ],
  }));
  transport.responders.set("model/list", () => ({
    data: [],
    nextCursor: null,
  }));
  transport.responders.set("thread/settings/update", () => ({}));
  const driver = new CodexAppServerDriver(transport, authority);

  const initialized = await driver.connect();
  const listed = await driver.threadList({ limit: 5 });
  const collaborationModes = await driver.collaborationModeList();
  const models = await driver.modelList({ limit: 50 });
  const settingsUpdate = await driver.threadSettingsUpdate({
    threadId: "thread-1",
    model: "gpt-5.4",
    effort: "high",
  });

  assert.equal(initialized.userAgent, "codex_cli_rs/0.144.1");
  assert.deepEqual(listed.data, []);
  assert.equal(collaborationModes.data[0]?.mode, "plan");
  assert.deepEqual(models, { data: [], nextCursor: null });
  assert.deepEqual(settingsUpdate, {});
  assert.equal(driver.state, "ready");
  assert.deepEqual(
    transport.sent
      .filter((message) => "method" in message)
      .map((message) => message.method),
    [
      "initialize",
      "thread/list",
      "collaborationMode/list",
      "model/list",
      "thread/settings/update",
    ],
  );
  await driver.close();
});

test("settings and token usage notifications retain browser-safe native authority", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({
    method: "thread/settings/updated",
    params: {
      threadId: "thread-settings-1",
      threadSettings: {
        cwd: "/home",
        approvalPolicy: "never",
        approvalsReviewer: "user",
        sandboxPolicy: { type: "dangerFullAccess" },
        activePermissionProfile: null,
        model: "gpt-5.4",
        modelProvider: "openai",
        serviceTier: null,
        effort: "high",
        summary: null,
        collaborationMode: {
          mode: "default",
          settings: {
            model: "gpt-5.4",
            reasoning_effort: "high",
            developer_instructions: null,
          },
        },
        multiAgentMode: "explicitRequestOnly",
        personality: null,
      },
    },
  });
  transport.emit({
    method: "thread/tokenUsage/updated",
    params: {
      threadId: "thread-settings-1",
      turnId: "turn-1",
      tokenUsage: {
        total: {
          totalTokens: 500,
          inputTokens: 400,
          cachedInputTokens: 100,
          outputTokens: 100,
          reasoningOutputTokens: 40,
        },
        last: {
          totalTokens: 100,
          inputTokens: 80,
          cachedInputTokens: 20,
          outputTokens: 20,
          reasoningOutputTokens: 10,
        },
        modelContextWindow: 200000,
      },
    },
  });
  await settle();

  assert.deepEqual(authority.events[0]?.payload.settings, {
    model: "gpt-5.4",
    modelProvider: "openai",
    effort: "high",
  });
  assert.deepEqual(authority.events[1]?.payload.usage, {
    total: {
      totalTokens: 500,
      inputTokens: 400,
      cachedInputTokens: 100,
      outputTokens: 100,
      reasoningOutputTokens: 40,
    },
    last: {
      totalTokens: 100,
      inputTokens: 80,
      cachedInputTokens: 20,
      outputTokens: 20,
      reasoningOutputTokens: 10,
    },
    modelContextWindow: 200000,
  });
  await driver.close();
});

test("driver exposes schema-checked native thread lifecycle calls", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  transport.responders.set("thread/archive", () => ({}));
  transport.responders.set("thread/delete", () => ({}));
  transport.responders.set("thread/unarchive", () => ({
    thread: {
      id: "thread-archive-1",
      extra: null,
      sessionId: "session-archive-1",
      forkedFromId: null,
      parentThreadId: null,
      preview: "archived thread",
      ephemeral: false,
      historyMode: "paginated",
      modelProvider: "openai",
      createdAt: 1,
      updatedAt: 2,
      recencyAt: 2,
      status: { type: "notLoaded" },
      path: "/tmp/thread.jsonl",
      cwd: "/home",
      cliVersion: "0.144.1",
      source: "appServer",
      threadSource: null,
      agentNickname: null,
      agentRole: null,
      gitInfo: null,
      name: null,
      turns: [],
    },
  }));
  const driver = new CodexAppServerDriver(transport, authority);

  await driver.connect();
  assert.deepEqual(
    await driver.threadArchive({ threadId: "thread-archive-1" }),
    {},
  );
  assert.deepEqual(
    await driver.threadDelete({ threadId: "thread-delete-1" }),
    {},
  );
  assert.equal(
    (await driver.threadUnarchive({ threadId: "thread-archive-1" })).thread.id,
    "thread-archive-1",
  );
  assert.deepEqual(
    transport.sent.slice(-3).map(({ method, params }) => ({ method, params })),
    [
      {
        method: "thread/archive",
        params: { threadId: "thread-archive-1" },
      },
      {
        method: "thread/delete",
        params: { threadId: "thread-delete-1" },
      },
      {
        method: "thread/unarchive",
        params: { threadId: "thread-archive-1" },
      },
    ],
  );
  transport.emit({
    method: "thread/archived",
    params: { threadId: "thread-archive-1" },
  });
  transport.emit({
    method: "thread/deleted",
    params: { threadId: "thread-delete-1" },
  });
  transport.emit({
    method: "thread/unarchived",
    params: { threadId: "thread-archive-1" },
  });
  await settle();
  assert.deepEqual(
    authority.events.map((event) => ({
      method: event.method,
      kind: event.kind,
      threadId: event.threadId,
    })),
    [
      {
        method: "thread/archived",
        kind: "thread_lifecycle",
        threadId: "thread-archive-1",
      },
      {
        method: "thread/deleted",
        kind: "thread_lifecycle",
        threadId: "thread-delete-1",
      },
      {
        method: "thread/unarchived",
        kind: "thread_lifecycle",
        threadId: "thread-archive-1",
      },
    ],
  );
  await driver.close();
});

test("driver preserves supplied agent message phase without inferring delta finality", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({
    method: "item/started",
    params: {
      threadId: "thread-phase-1",
      turnId: "turn-phase-1",
      startedAtMs: 1,
      item: {
        type: "agentMessage",
        id: "commentary-1",
        text: "I am checking the repository.",
        phase: "commentary",
        memoryCitation: null,
      },
    },
  });
  transport.emit({
    method: "item/agentMessage/delta",
    params: {
      threadId: "thread-phase-1",
      turnId: "turn-phase-1",
      itemId: "commentary-1",
      delta: "More progress",
    },
  });
  transport.emit({
    method: "item/completed",
    params: {
      threadId: "thread-phase-1",
      turnId: "turn-phase-1",
      completedAtMs: 2,
      item: {
        type: "agentMessage",
        id: "final-1",
        text: "The task is complete.",
        phase: "final_answer",
        memoryCitation: null,
      },
    },
  });
  await settle();

  assert.deepEqual(
    authority.events.map((event) => ({
      kind: event.kind,
      itemId: event.itemId,
      text: event.payload.text,
      messagePhase: event.payload.messagePhase,
    })),
    [
      {
        kind: "item_lifecycle",
        itemId: "commentary-1",
        text: "I am checking the repository.",
        messagePhase: "commentary",
      },
      {
        kind: "assistant_text_delta",
        itemId: "commentary-1",
        text: "More progress",
        messagePhase: undefined,
      },
      {
        kind: "item_lifecycle",
        itemId: "final-1",
        text: "The task is complete.",
        messagePhase: "final_answer",
      },
    ],
  );
  await driver.close();
});

test("Rust authority can reject an incompatible runtime before mutation", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  authority.accepted = false;
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority);

  await assert.rejects(driver.connect(), /rejected Codex app-server handshake/);
  assert.equal(driver.state, "incompatible");
  assert.equal(transport.closed, true);
  assert.equal(
    transport.sent.some((message) => message.method === "thread/list"),
    false,
  );
});

test("known dynamic tools are resolved through the leased Rust callback", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  authority.resolution = {
    type: "success",
    result: {
      contentItems: [{ type: "inputText", text: "crew result" }],
      success: true,
    },
  };
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({
    id: "tool-1",
    method: "item/tool/call",
    params: {
      threadId: "thread-1",
      turnId: "turn-1",
      callId: "call-1",
      namespace: "rusty_crew",
      tool: "send_agent_message",
      arguments: { recipient: "agent-b", body: "hello" },
    },
  });
  await settle();

  assert.deepEqual(authority.serverRequests, ["item/tool/call"]);
  assert.deepEqual(transport.sent.at(-1), {
    id: "tool-1",
    result: {
      contentItems: [{ type: "inputText", text: "crew result" }],
      success: true,
    },
  });
  await driver.close();
});

test("pending server requests do not block client responses on the same socket", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  let resolveTool: ((value: ServerRequestResolution) => void) | undefined;
  authority.resolver = () =>
    new Promise<ServerRequestResolution>((resolve) => {
      resolveTool = resolve;
    });
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({
    id: "tool-blocked",
    method: "item/tool/call",
    params: {
      threadId: "thread-1",
      turnId: "turn-1",
      callId: "call-blocked",
      namespace: "rusty_crew",
      tool: "agent_round",
      arguments: { recipient: "agent-b", body: "wait for reply" },
    },
  });
  await settle();
  assert.equal(typeof resolveTool, "function");

  const listed = driver.threadList({ limit: 5 });
  await settle();
  const listRequest = transport.sent.find(
    (message) => message.method === "thread/list",
  );
  assert.notEqual(listRequest?.id, undefined);
  transport.emit({
    id: listRequest?.id,
    result: { data: [], nextCursor: null, backwardsCursor: null },
  });
  assert.deepEqual((await listed).data, []);
  assert.equal(
    transport.sent.some((message) => message.id === "tool-blocked"),
    false,
  );

  resolveTool?.({
    type: "success",
    result: {
      contentItems: [{ type: "inputText", text: "round reply" }],
      success: true,
    },
  });
  await settle();
  assert.equal(
    transport.sent.some(
      (message) => message.id === "tool-blocked" && "result" in message,
    ),
    true,
  );
  await driver.close();
});

test("unknown requests fail closed while unknown notifications remain evidence", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({ id: 44, method: "future/approval", params: { value: 1 } });
  transport.emit({ method: "future/notice", params: { value: 2 } });
  transport.emit({
    method: "future/redacted",
    params: { access_token: "must-not-survive", visible: "kept" },
  });
  await settle();

  assert.equal(authority.events[0]?.kind, "unsupported_server_request");
  assert.equal(authority.events[1]?.kind, "unknown_native_notification");
  assert.match(authority.events[2]?.rawDetail.json ?? "", /\[REDACTED\]/);
  assert.doesNotMatch(
    authority.events[2]?.rawDetail.json ?? "",
    /must-not-survive/,
  );
  assert.deepEqual(transport.sent.at(-1), {
    id: 44,
    error: {
      code: -32601,
      message: "unsupported app-server request future/approval",
    },
  });
  await driver.close();
});

test("every interactive request family is brokered and schema checked", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  authority.resolutions.set("item/commandExecution/requestApproval", {
    type: "success",
    result: { decision: "decline" },
  });
  authority.resolutions.set("item/fileChange/requestApproval", {
    type: "success",
    result: { decision: "decline" },
  });
  authority.resolutions.set("item/tool/requestUserInput", {
    type: "success",
    result: { answers: {} },
  });
  authority.resolutions.set("mcpServer/elicitation/request", {
    type: "success",
    result: { action: "decline", content: null, _meta: null },
  });
  authority.resolutions.set("item/permissions/requestApproval", {
    type: "success",
    result: { permissions: {}, scope: "turn", strictAutoReview: false },
  });
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  const requests = [
    {
      id: "command",
      method: "item/commandExecution/requestApproval",
      params: {
        threadId: "thread",
        turnId: "turn",
        itemId: "item",
        startedAtMs: 1,
        environmentId: null,
      },
    },
    {
      id: "file",
      method: "item/fileChange/requestApproval",
      params: {
        threadId: "thread",
        turnId: "turn",
        itemId: "item",
        startedAtMs: 1,
      },
    },
    {
      id: "input",
      method: "item/tool/requestUserInput",
      params: {
        threadId: "thread",
        turnId: "turn",
        itemId: "item",
        questions: [],
        autoResolutionMs: null,
      },
    },
    {
      id: "mcp",
      method: "mcpServer/elicitation/request",
      params: {
        threadId: "thread",
        turnId: "turn",
        serverName: "server",
        mode: "form",
        _meta: null,
        message: "choose",
        requestedSchema: { type: "object", properties: {} },
      },
    },
    {
      id: "permissions",
      method: "item/permissions/requestApproval",
      params: {
        threadId: "thread",
        turnId: "turn",
        itemId: "item",
        environmentId: null,
        startedAtMs: 1,
        cwd: "/tmp",
        reason: null,
        permissions: { network: null, fileSystem: null },
      },
    },
  ];
  for (const request of requests) transport.emit(request);
  await settle();

  assert.deepEqual(
    authority.serverRequests,
    requests.map((request) => request.method),
  );
  for (const request of requests) {
    const response = transport.sent.find(
      (message) => message.id === request.id,
    );
    assert.equal(response?.error, undefined, request.method);
    assert.notEqual(response?.result, undefined, request.method);
  }
  await driver.close();
});

test("notification order and duplicate responses are explicit", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  transport.responders.set("thread/list", () => ({
    data: [],
    nextCursor: null,
    backwardsCursor: null,
  }));
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();
  await driver.threadList({});
  const listRequest = transport.sent.find(
    (message) => message.method === "thread/list",
  );
  transport.emit({ method: "future/one", params: {} });
  transport.emit({ method: "future/two", params: {} });
  transport.emit({
    id: listRequest?.id,
    result: { data: [], nextCursor: null, backwardsCursor: null },
  });
  await settle();

  assert.deepEqual(
    authority.events.map((event) => [event.method, event.transportSequence]),
    [
      ["future/one", 3],
      ["future/two", 4],
    ],
  );
  assert.equal(authority.faults.at(-1)?.reasonCode, "duplicate_response");
  assert.equal(authority.faults.at(-1)?.fatal, false);
  await driver.close();
});

test("pending capacity, abort, timeout, and disconnect are bounded", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority, {
    requestTimeoutMs: 25,
    maxPendingRequests: 1,
  });
  await driver.connect();

  const controller = new AbortController();
  const pending = driver.threadList({}, controller.signal);
  await assert.rejects(driver.threadRead({ threadId: "x" }), /capacity/);
  controller.abort(new Error("operator cancelled"));
  await assert.rejects(pending, /operator cancelled/);

  const timedOut = driver.threadList({});
  await assert.rejects(timedOut, /timed out/);

  const disconnected = driver.threadList({});
  transport.disconnect();
  await assert.rejects(disconnected, /disconnected/);
  await settle();
  assert.equal(authority.disconnects.length, 1);
  assert.equal(authority.disconnects[0]?.pendingClientRequestIds.length, 1);
});

test("timeouts are reported once and never retried by the TypeScript driver", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  const driver = new CodexAppServerDriver(transport, authority, {
    requestTimeoutMs: 20,
  });
  await driver.connect();

  await assert.rejects(driver.threadList({}), /timed out/);
  await new Promise((resolve) => setTimeout(resolve, 30));
  assert.equal(
    transport.sent.filter((message) => message.method === "thread/list").length,
    1,
  );
  await driver.close();
});

test("disconnect exposes pending native interactions and suppresses late replies", async () => {
  const transport = new FakeTransport();
  const authority = new FakeAuthority();
  configureInitialize(transport);
  let finishResolution: ((value: ServerRequestResolution) => void) | undefined;
  authority.resolver = () =>
    new Promise((resolve) => {
      finishResolution = resolve;
    });
  const driver = new CodexAppServerDriver(transport, authority);
  await driver.connect();

  transport.emit({
    id: "pending-tool",
    method: "item/tool/call",
    params: {
      threadId: "thread-1",
      turnId: "turn-1",
      callId: "call-1",
      namespace: "rusty_crew",
      tool: "agent_round",
      arguments: { recipient: "agent-b", body: "hello" },
    },
  });
  await settle();
  transport.disconnect("controller lease lost");
  await settle();

  assert.deepEqual(authority.disconnects[0]?.pendingServerRequestIds, [
    "pending-tool",
  ]);
  finishResolution?.({
    type: "success",
    result: {
      contentItems: [{ type: "inputText", text: "late" }],
      success: true,
    },
  });
  await settle();
  assert.equal(
    transport.sent.some(
      (message) => message.id === "pending-tool" && "result" in message,
    ),
    false,
  );
});

test("codec rejects malformed known messages but preserves future methods", () => {
  const codec = new CodexProtocolCodec();
  assert.throws(
    () => codec.decode(JSON.stringify({ method: "turn/started", params: {} })),
    /validation failed/,
  );
  assert.deepEqual(
    codec.decode(JSON.stringify({ method: "future/notice", params: { x: 1 } })),
    {
      type: "unknown_notification",
      method: "future/notice",
      params: { x: 1 },
    },
  );
});
