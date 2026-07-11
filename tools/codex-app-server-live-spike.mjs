import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join, relative } from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import WebSocket from "ws";

const socketPath =
  process.env.CODEX_APP_SERVER_SOCKET ??
  "/run/user/1001/codex-app-server/app-server.sock";
const scratchRoot =
  process.env.CODEX_APP_SERVER_SPIKE_ROOT ??
  `/home/agent/.cache/rusty-crew/codex-app-server-spikes/${Date.now()}`;
const restartService = process.env.CODEX_APP_SERVER_RESTART_SERVICE === "1";
const keepScratch = process.env.CODEX_APP_SERVER_KEEP_SCRATCH === "1";
const requestTimeoutMs = Number(
  process.env.CODEX_APP_SERVER_REQUEST_TIMEOUT_MS ?? 30_000,
);
const turnTimeoutMs = Number(
  process.env.CODEX_APP_SERVER_TURN_TIMEOUT_MS ?? 300_000,
);

class AppServerClient {
  #nextRequestId = 1;
  #pending = new Map();
  #notificationWaiters = [];
  #closed = false;

  constructor(socket, label) {
    this.socket = socket;
    this.label = label;
    this.notifications = [];
    this.serverRequests = [];
    this.dynamicToolCalls = [];
    this.unknownServerRequests = [];
  }

  static async connect(socketPathValue, label) {
    // App-server's UDS listener rejects the default permessage-deflate offer
    // produced by ws 8.21. Keep compression disabled at this boundary.
    const socket = new WebSocket(`ws+unix://${socketPathValue}:/`, {
      perMessageDeflate: false,
      handshakeTimeout: requestTimeoutMs,
    });
    const client = new AppServerClient(socket, label);
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error(`${label}: websocket open timed out`)),
        requestTimeoutMs,
      );
      socket.once("open", () => {
        clearTimeout(timer);
        resolve();
      });
      socket.once("error", (error) => {
        clearTimeout(timer);
        reject(error);
      });
    });
    socket.on("message", (data) => client.#receive(data));
    socket.on("close", () => client.#closePending("websocket closed"));
    socket.on("error", (error) => client.#closePending(error.message));
    return client;
  }

  async initialize() {
    return this.request("initialize", {
      clientInfo: {
        name: "rusty_crew_live_spike",
        title: "Rusty Crew Codex App-Server Live Spike",
        version: "0.1.0",
      },
      capabilities: { experimentalApi: true },
    });
  }

  request(method, params, timeoutMs = requestTimeoutMs) {
    assert.equal(this.#closed, false, `${this.label}: client is closed`);
    const id = this.#nextRequestId++;
    const promise = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(
          new Error(`${this.label}: ${method} timed out after ${timeoutMs}ms`),
        );
      }, timeoutMs);
      this.#pending.set(id, { method, resolve, reject, timer });
    });
    this.socket.send(JSON.stringify({ method, id, params }));
    return promise;
  }

  waitForNotification(
    method,
    predicate = () => true,
    timeoutMs = turnTimeoutMs,
  ) {
    const existing = this.notifications.find(
      (message) => message.method === method && predicate(message.params),
    );
    if (existing !== undefined) return Promise.resolve(existing.params);
    return new Promise((resolve, reject) => {
      const waiter = {
        method,
        predicate,
        resolve,
        reject,
        timer: undefined,
      };
      waiter.timer = setTimeout(() => {
        const index = this.#notificationWaiters.indexOf(waiter);
        if (index >= 0) this.#notificationWaiters.splice(index, 1);
        reject(
          new Error(
            `${this.label}: notification ${method} timed out after ${timeoutMs}ms`,
          ),
        );
      }, timeoutMs);
      this.#notificationWaiters.push(waiter);
    });
  }

  async close() {
    if (this.#closed) return;
    this.#closed = true;
    await new Promise((resolve) => {
      const timer = setTimeout(resolve, 1_000);
      this.socket.once("close", () => {
        clearTimeout(timer);
        resolve();
      });
      this.socket.close();
    });
    this.#closePending("client closed");
  }

  #receive(data) {
    const message = JSON.parse(data.toString());
    if (message.method !== undefined && message.id !== undefined) {
      this.#handleServerRequest(message).catch((error) => {
        this.socket.send(
          JSON.stringify({
            id: message.id,
            error: { code: -32603, message: error.message },
          }),
        );
      });
      return;
    }
    if (message.id !== undefined) {
      const pending = this.#pending.get(message.id);
      if (pending === undefined) return;
      clearTimeout(pending.timer);
      this.#pending.delete(message.id);
      if (message.error !== undefined) {
        pending.reject(
          new Error(
            `${this.label}: ${pending.method} failed: ${JSON.stringify(message.error)}`,
          ),
        );
      } else {
        pending.resolve(message.result);
      }
      return;
    }
    if (message.method === undefined) return;
    this.notifications.push(message);
    for (const waiter of [...this.#notificationWaiters]) {
      if (
        waiter.method !== message.method ||
        !waiter.predicate(message.params)
      ) {
        continue;
      }
      clearTimeout(waiter.timer);
      this.#notificationWaiters.splice(
        this.#notificationWaiters.indexOf(waiter),
        1,
      );
      waiter.resolve(message.params);
    }
  }

  async #handleServerRequest(message) {
    this.serverRequests.push({
      method: message.method,
      params: message.params,
    });
    if (message.method === "item/tool/call") {
      const { namespace, tool, arguments: args } = message.params;
      if (namespace !== "rusty_crew" || tool !== "echo_probe") {
        this.unknownServerRequests.push(message.method);
        this.#sendError(
          message.id,
          -32601,
          `unknown dynamic tool ${namespace}.${tool}`,
        );
        return;
      }
      assert.equal(
        typeof args?.token,
        "string",
        "dynamic tool token is required",
      );
      this.dynamicToolCalls.push({ ...message.params });
      if (args.token.startsWith("pending-kill-")) {
        // Deliberately leave this request unresolved so the live spike can
        // establish hard-restart behavior for an in-flight tool callback.
        return;
      }
      this.#sendResult(message.id, {
        contentItems: [
          {
            type: "inputText",
            text: `RUSTY_CREW_DYNAMIC_ACK:${args.token}`,
          },
        ],
        success: true,
      });
      return;
    }
    if (message.method === "item/tool/requestUserInput") {
      const answers = Object.fromEntries(
        message.params.questions.map((question) => [
          question.id,
          {
            answers: [
              question.options?.[0]?.label ?? "Rusty Crew live spike answer",
            ],
          },
        ]),
      );
      this.#sendResult(message.id, { answers });
      return;
    }
    if (
      message.method === "item/commandExecution/requestApproval" ||
      message.method === "item/fileChange/requestApproval"
    ) {
      this.#sendResult(message.id, { decision: "decline" });
      return;
    }
    if (message.method === "item/permissions/requestApproval") {
      this.#sendResult(message.id, {
        permissions: {},
        scope: "turn",
        strictAutoReview: false,
      });
      return;
    }
    if (message.method === "mcpServer/elicitation/request") {
      this.#sendResult(message.id, { action: "decline", content: null });
      return;
    }
    this.unknownServerRequests.push(message.method);
    this.#sendError(
      message.id,
      -32601,
      `unsupported server request ${message.method}`,
    );
  }

  #sendResult(id, result) {
    this.socket.send(JSON.stringify({ id, result }));
  }

  #sendError(id, code, message) {
    this.socket.send(JSON.stringify({ id, error: { code, message } }));
  }

  #closePending(reason) {
    if (this.#closed === false) this.#closed = true;
    for (const pending of this.#pending.values()) {
      clearTimeout(pending.timer);
      pending.reject(new Error(`${this.label}: ${reason}`));
    }
    this.#pending.clear();
    for (const waiter of this.#notificationWaiters) {
      clearTimeout(waiter.timer);
      waiter.reject(new Error(`${this.label}: ${reason}`));
    }
    this.#notificationWaiters = [];
  }
}

function createScratchRepository(root) {
  rmSync(root, { recursive: true, force: true });
  mkdirSync(root, { recursive: true });
  writeFileSync(
    join(root, "AGENTS.md"),
    "# Scratch Agent Guidance\n\nKeep changes inside this repository. Run `npm test` before finishing.\n",
  );
  writeFileSync(
    join(root, "package.json"),
    `${JSON.stringify(
      {
        name: "rusty-crew-codex-app-server-spike",
        private: true,
        type: "module",
        scripts: { test: "node --test" },
      },
      null,
      2,
    )}\n`,
  );
  writeFileSync(
    join(root, "math.js"),
    'export function add(left, right) {\n  throw new Error("not implemented");\n}\n',
  );
  writeFileSync(
    join(root, "math.test.js"),
    'import assert from "node:assert/strict";\nimport test from "node:test";\nimport { add } from "./math.js";\n\ntest("add", () => {\n  assert.equal(add(2, 3), 5);\n});\n',
  );
  execFileSync("git", ["init", "-q"], { cwd: root });
  execFileSync("git", ["config", "user.name", "Rusty Crew Spike"], {
    cwd: root,
  });
  execFileSync("git", ["config", "user.email", "spike@rusty-crew.local"], {
    cwd: root,
  });
  execFileSync("git", ["add", "."], { cwd: root });
  execFileSync("git", ["commit", "-qm", "Create spike fixture"], { cwd: root });
}

function dynamicTools() {
  return [
    {
      type: "namespace",
      name: "rusty_crew",
      description: "Identity-bound Rusty Crew coordination probe tools.",
      tools: [
        {
          type: "function",
          name: "echo_probe",
          description:
            "Return a deterministic acknowledgement for a live integration token.",
          deferLoading: false,
          inputSchema: {
            type: "object",
            properties: { token: { type: "string" } },
            required: ["token"],
            additionalProperties: false,
          },
        },
      ],
    },
  ];
}

async function startTurn(client, threadId, text, overrides = {}) {
  const response = await client.request("turn/start", {
    threadId,
    input: [{ type: "text", text }],
    approvalPolicy: "never",
    sandboxPolicy: { type: "dangerFullAccess" },
    effort: "medium",
    ...overrides,
  });
  const turnId = response.turn.id;
  const terminal = await client.waitForNotification(
    "turn/completed",
    (params) => params.turn.id === turnId,
  );
  return terminal.turn;
}

function allCompletedItems(client) {
  return client.notifications
    .filter((message) => message.method === "item/completed")
    .map((message) => message.params.item);
}

function scratchAcceptance(root) {
  const test = spawnSync("npm", ["test"], {
    cwd: root,
    encoding: "utf8",
  });
  return {
    accepted:
      test.status === 0 &&
      /return/.test(readFileSync(join(root, "math.js"), "utf8")) &&
      existsSync(join(root, "SPIKE_NOTES.md")),
    test,
  };
}

function itemTypes(client) {
  return [
    ...new Set(
      client.notifications
        .filter((message) => message.method === "item/completed")
        .map((message) => message.params.item?.type)
        .filter(Boolean),
    ),
  ].sort();
}

function waitForSocket(path, timeoutMs = 20_000) {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    while (Date.now() < deadline) {
      if (existsSync(path) && statSync(path).isSocket()) return;
      await delay(100);
    }
    throw new Error(`socket ${path} did not reappear after service restart`);
  })();
}

async function waitForCondition(predicate, description, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await delay(25);
  }
  throw new Error(`${description} did not become true after ${timeoutMs}ms`);
}

function restartDedicatedService() {
  execFileSync("systemctl", ["--user", "restart", "codex-app-server.service"], {
    env: { ...process.env, XDG_RUNTIME_DIR: "/run/user/1001" },
    stdio: "inherit",
  });
}

async function killDedicatedServiceDuringTurn() {
  const environment = { ...process.env, XDG_RUNTIME_DIR: "/run/user/1001" };
  const mainPid = () =>
    execFileSync(
      "systemctl",
      [
        "--user",
        "show",
        "--property=MainPID",
        "--value",
        "codex-app-server.service",
      ],
      { env: environment },
    )
      .toString()
      .trim();
  const previousPid = mainPid();
  execFileSync(
    "systemctl",
    [
      "--user",
      "kill",
      "--kill-whom=all",
      "--signal=SIGKILL",
      "codex-app-server.service",
    ],
    { env: environment },
  );
  await waitForCondition(() => {
    try {
      const currentPid = mainPid();
      return (
        currentPid !== "0" &&
        currentPid !== previousPid &&
        existsSync(socketPath)
      );
    } catch {
      return false;
    }
  }, "app-server replacement process");
  await delay(500);
  return { previousPid, replacementPid: mainPid() };
}

function sha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

function generatedProtocolFingerprint(kind) {
  const outputRoot = join(
    "/home/agent/.cache/rusty-crew/codex-app-server-protocol",
    `${process.pid}-${kind}-${Date.now()}`,
  );
  mkdirSync(outputRoot, { recursive: true });
  try {
    const command =
      kind === "typescript" ? "generate-ts" : "generate-json-schema";
    execFileSync("codex", [
      "app-server",
      command,
      "--experimental",
      "--out",
      outputRoot,
    ]);
    const files = readdirSync(outputRoot, {
      recursive: true,
      withFileTypes: true,
    })
      .filter((entry) => entry.isFile())
      .map((entry) => join(entry.parentPath, entry.name))
      .sort((left, right) => left.localeCompare(right));
    const hash = createHash("sha256");
    for (const file of files) {
      hash.update(relative(outputRoot, file));
      hash.update("\0");
      hash.update(readFileSync(file));
      hash.update("\0");
    }
    return { fileCount: files.length, sha256: hash.digest("hex") };
  } finally {
    rmSync(outputRoot, { recursive: true, force: true });
  }
}

function protocolFingerprints() {
  const launcher = execFileSync("bash", [
    "-lc",
    "readlink -f $(command -v codex)",
  ])
    .toString()
    .trim();
  const serverPid = execFileSync("pgrep", [
    "-f",
    `/vendor/.*/codex app-server --listen unix://${socketPath}`,
  ])
    .toString()
    .trim()
    .split("\n")[0];
  const executable = execFileSync("readlink", ["-f", `/proc/${serverPid}/exe`])
    .toString()
    .trim();
  return {
    cliVersion: execFileSync("codex", ["--version"]).toString().trim(),
    launcher,
    launcherSha256: sha256(launcher),
    executable,
    executableSha256: sha256(executable),
  };
}

async function main() {
  assert.equal(
    existsSync(socketPath),
    true,
    `missing app-server socket ${socketPath}`,
  );
  createScratchRepository(scratchRoot);
  const runId = randomUUID();
  const firstToken = `first-${runId}`;
  const resumedToken = `resumed-${runId}`;
  let firstClient;
  let resumedClient;
  let recoveryClient;
  let firstDynamicToolCalls = [];
  let firstItemTypes = [];
  let firstServerRequestMethods = [];
  try {
    firstClient = await AppServerClient.connect(
      socketPath,
      "initial-controller",
    );
    const initialize = await firstClient.initialize();
    const models = await firstClient.request("model/list", {
      limit: 100,
      includeHidden: false,
    });
    const selectedModel =
      models.data.find((candidate) => candidate.isDefault) ?? models.data[0];
    assert.notEqual(
      selectedModel,
      undefined,
      "app-server advertised no models",
    );
    const started = await firstClient.request("thread/start", {
      model: selectedModel.model,
      cwd: scratchRoot,
      approvalPolicy: "never",
      sandbox: "danger-full-access",
      serviceName: "rusty_crew_live_spike",
      sessionStartSource: "startup",
      ephemeral: false,
      // Omission selects the account's mutable default environment, while an
      // empty list removes execution tools. Bind service agents explicitly to
      // the machine-local executor and cwd.
      environments: [{ environmentId: "local", cwd: scratchRoot }],
      dynamicTools: dynamicTools(),
    });
    const threadId = started.thread.id;
    const mcpStatus = await firstClient.request("mcpServerStatus/list", {
      threadId,
      detail: "full",
      limit: 100,
    });
    const firstTurn = await startTurn(
      firstClient,
      threadId,
      [
        `Call the dynamic tool rusty_crew.echo_probe exactly once with token ${firstToken}.`,
        "Use the Den MCP task-reading tool to read Rusty Crew task 5517 and obtain its exact title. Do not modify Den.",
        "Then inspect this scratch repository, implement add() in math.js, and run npm test.",
        "Create SPIKE_NOTES.md containing the dynamic acknowledgement and the exact Den task title.",
        "Do not alter files outside the current repository. Finish with a concise result.",
      ].join("\n"),
    );
    assert.equal(firstTurn.status, "completed", "first turn did not complete");
    assert.equal(
      firstClient.dynamicToolCalls.some(
        (call) => call.arguments.token === firstToken,
      ),
      true,
      "first turn did not invoke the dynamic tool",
    );
    const codeTurnIds = [firstTurn.id];
    let acceptance = scratchAcceptance(scratchRoot);
    if (!acceptance.accepted) {
      const continuation = await startTurn(
        firstClient,
        threadId,
        [
          "The previous turn ended before the requested repository work met acceptance.",
          "Finish the original task now: read Den task 5517 if you have not already, implement math.js, create SPIKE_NOTES.md with the exact task title and prior dynamic acknowledgement, and run npm test.",
          "Do not call rusty_crew.echo_probe again and do not edit outside this scratch repository.",
        ].join("\n"),
      );
      assert.equal(continuation.status, "completed");
      codeTurnIds.push(continuation.id);
      acceptance = scratchAcceptance(scratchRoot);
    }
    if (acceptance.test.stdout) process.stdout.write(acceptance.test.stdout);
    if (acceptance.test.stderr) process.stderr.write(acceptance.test.stderr);
    assert.equal(
      acceptance.accepted,
      true,
      "scratch acceptance remained incomplete",
    );
    assert.match(readFileSync(join(scratchRoot, "math.js"), "utf8"), /return/);
    assert.equal(existsSync(join(scratchRoot, "SPIKE_NOTES.md")), true);
    const firstItems = allCompletedItems(firstClient);
    const denMcpCalls = firstItems.filter(
      (item) =>
        item?.type === "mcpToolCall" &&
        `${item.server ?? ""} ${item.tool ?? ""}`.toLowerCase().includes("den"),
    );
    assert.notEqual(denMcpCalls.length, 0, "first turn did not use Den MCP");
    firstDynamicToolCalls = [...firstClient.dynamicToolCalls];
    firstItemTypes = itemTypes(firstClient);
    firstServerRequestMethods = [
      ...new Set(firstClient.serverRequests.map((request) => request.method)),
    ].sort();

    await firstClient.close();
    firstClient = undefined;
    if (restartService) {
      restartDedicatedService();
      await waitForSocket(socketPath);
      await delay(500);
    }

    resumedClient = await AppServerClient.connect(
      socketPath,
      "resumed-controller",
    );
    const resumedInitialize = await resumedClient.initialize();
    const resumed = await resumedClient.request("thread/resume", {
      threadId,
      cwd: scratchRoot,
      approvalPolicy: "never",
      sandbox: "danger-full-access",
    });
    assert.equal(
      resumed.thread.id,
      threadId,
      "thread identity changed on resume",
    );
    const secondTurn = await startTurn(
      resumedClient,
      threadId,
      `Call rusty_crew.echo_probe exactly once with token ${resumedToken}. Then read math.js and state the value of add(7, 8). Do not edit files.`,
    );
    assert.equal(
      secondTurn.status,
      "completed",
      "resumed turn did not complete",
    );
    assert.equal(
      resumedClient.dynamicToolCalls.some(
        (call) => call.arguments.token === resumedToken,
      ),
      true,
      "dynamic tools were not restored on thread resume",
    );

    const collaborationModes = await resumedClient.request(
      "collaborationMode/list",
      {},
    );
    const planMode = collaborationModes.data.find(
      (candidate) => candidate.mode === "plan",
    );
    assert.notEqual(
      planMode,
      undefined,
      "app-server did not advertise Plan mode",
    );
    const inputTurn = await startTurn(
      resumedClient,
      threadId,
      "Use request_user_input exactly once to ask whether the spike should continue, with Continue as the recommended first option. After receiving the answer, report it and finish.",
      {
        collaborationMode: {
          mode: "plan",
          settings: {
            model: planMode.model ?? selectedModel.model,
            reasoning_effort: planMode.reasoning_effort,
            developer_instructions: null,
          },
        },
      },
    );
    assert.equal(inputTurn.status, "completed", "requestUserInput turn failed");
    assert.equal(
      resumedClient.serverRequests.some(
        (request) => request.method === "item/tool/requestUserInput",
      ),
      true,
      "requestUserInput callback was not exercised",
    );

    const defaultCollaborationMode = {
      mode: "default",
      settings: {
        model: selectedModel.model,
        reasoning_effort: selectedModel.defaultReasoningEffort,
        developer_instructions: null,
      },
    };
    const approvalThread = await resumedClient.request("thread/start", {
      model: selectedModel.model,
      cwd: scratchRoot,
      approvalPolicy: "untrusted",
      sandbox: "read-only",
      serviceName: "rusty_crew_live_spike",
      ephemeral: true,
      environments: [{ environmentId: "local", cwd: scratchRoot }],
    });
    const approvalTurn = await startTurn(
      resumedClient,
      approvalThread.thread.id,
      "Attempt to run exactly `bash -lc 'printf APPROVAL_PROBE > approval-probe.txt'` using command execution. Do not use a file editing tool. If the client declines approval, report that and finish.",
      {
        approvalPolicy: "untrusted",
        sandboxPolicy: { type: "readOnly" },
        collaborationMode: defaultCollaborationMode,
      },
    );
    assert.equal(approvalTurn.status, "completed");
    assert.equal(
      resumedClient.serverRequests.some(
        (request) => request.method === "item/commandExecution/requestApproval",
      ),
      true,
      "command approval callback was not exercised",
    );
    assert.equal(
      existsSync(join(scratchRoot, "approval-probe.txt")),
      false,
      "declined approval unexpectedly wrote the probe file",
    );

    const permissionTurn = await startTurn(
      resumedClient,
      threadId,
      `Use request_permissions exactly once to request write access to ${scratchRoot}. After the client responds, report the result and finish without editing files.`,
      {
        approvalPolicy: {
          granular: {
            mcp_elicitations: true,
            rules: false,
            sandbox_approval: true,
            request_permissions: true,
            skill_approval: false,
          },
        },
        sandboxPolicy: { type: "readOnly" },
        collaborationMode: defaultCollaborationMode,
      },
    );
    assert.equal(permissionTurn.status, "completed");

    const subagentTurn = await startTurn(
      resumedClient,
      threadId,
      "Delegate exactly once to a subagent to inspect math.js and report whether add() is implemented correctly. Do not edit files.",
      {
        effort: "ultra",
        collaborationMode: {
          mode: "default",
          settings: {
            model: selectedModel.model,
            reasoning_effort: "ultra",
            developer_instructions: null,
          },
        },
      },
    );
    assert.equal(subagentTurn.status, "completed");

    const steerStart = await resumedClient.request("turn/start", {
      threadId,
      input: [
        {
          type: "text",
          text: "Run sleep 5, then reply with STEER_ORIGINAL and nothing else.",
        },
      ],
      approvalPolicy: "never",
      sandboxPolicy: { type: "dangerFullAccess" },
    });
    await resumedClient.waitForNotification(
      "turn/started",
      (params) => params.turn.id === steerStart.turn.id,
    );
    await delay(500);
    const steered = await resumedClient.request("turn/steer", {
      threadId,
      expectedTurnId: steerStart.turn.id,
      input: [
        {
          type: "text",
          text: "Instead reply with STEER_ACCEPTED and nothing else.",
        },
      ],
    });
    assert.equal(steered.turnId, steerStart.turn.id);
    const steerTerminal = await resumedClient.waitForNotification(
      "turn/completed",
      (params) => params.turn.id === steerStart.turn.id,
    );
    assert.equal(steerTerminal.turn.status, "completed");

    const interruptStart = await resumedClient.request("turn/start", {
      threadId,
      input: [
        {
          type: "text",
          text: "Run sleep 30, then reply with INTERRUPT_MISSED.",
        },
      ],
      approvalPolicy: "never",
      sandboxPolicy: { type: "dangerFullAccess" },
    });
    await resumedClient.waitForNotification(
      "turn/started",
      (params) => params.turn.id === interruptStart.turn.id,
    );
    await delay(500);
    await resumedClient.request("turn/interrupt", {
      threadId,
      turnId: interruptStart.turn.id,
    });
    const interruptTerminal = await resumedClient.waitForNotification(
      "turn/completed",
      (params) => params.turn.id === interruptStart.turn.id,
    );
    assert.equal(interruptTerminal.turn.status, "interrupted");

    const knownTurnIds = new Set(
      resumedClient.notifications
        .filter((message) => message.method === "turn/started")
        .map((message) => message.params.turn.id),
    );
    await resumedClient.request("thread/compact/start", { threadId });
    const compactStarted = await resumedClient.waitForNotification(
      "turn/started",
      (params) => !knownTurnIds.has(params.turn.id),
    );
    const compactTerminal = await resumedClient.waitForNotification(
      "turn/completed",
      (params) => params.turn.id === compactStarted.turn.id,
    );

    const pendingKillToken = `pending-kill-${runId}`;
    const pendingTurn = await resumedClient.request("turn/start", {
      threadId,
      input: [
        {
          type: "text",
          text: `Call rusty_crew.echo_probe exactly once with token ${pendingKillToken}, then report its result.`,
        },
      ],
      approvalPolicy: "never",
      sandboxPolicy: { type: "dangerFullAccess" },
    });
    await waitForCondition(
      () =>
        resumedClient.dynamicToolCalls.some(
          (call) => call.arguments.token === pendingKillToken,
        ),
      "pending dynamic-tool request",
    );
    const hardRestart = await killDedicatedServiceDuringTurn();
    recoveryClient = await AppServerClient.connect(
      socketPath,
      "recovery-controller",
    );
    const recoveryInitialize = await recoveryClient.initialize();
    const recovered = await recoveryClient.request("thread/resume", {
      threadId,
      cwd: scratchRoot,
      approvalPolicy: "never",
      sandbox: "danger-full-access",
    });
    assert.equal(recovered.thread.id, threadId);
    const threadRead = await recoveryClient.request("thread/read", {
      threadId,
      includeTurns: true,
    });
    const killedTurn = threadRead.thread.turns.find(
      (turn) => turn.id === pendingTurn.turn.id,
    );
    assert.notEqual(
      killedTurn,
      undefined,
      "killed pending turn disappeared from history",
    );
    assert.notEqual(
      killedTurn.status,
      "inProgress",
      "killed pending turn remained falsely active",
    );
    const report = {
      runId,
      scratchRoot,
      socketPath,
      restartService,
      protocol: protocolFingerprints(),
      generatedProtocol: {
        typescript: generatedProtocolFingerprint("typescript"),
        jsonSchema: generatedProtocolFingerprint("json-schema"),
      },
      selectedModel: {
        id: selectedModel.id,
        model: selectedModel.model,
        defaultReasoningEffort: selectedModel.defaultReasoningEffort,
      },
      initialize,
      resumedInitialize,
      recoveryInitialize,
      threadId,
      firstTurnId: firstTurn.id,
      codeTurnIds,
      secondTurnId: secondTurn.id,
      inputTurnId: inputTurn.id,
      approvalThreadId: approvalThread.thread.id,
      approvalTurnId: approvalTurn.id,
      permissionTurnId: permissionTurn.id,
      subagentTurnId: subagentTurn.id,
      steerTurnId: steerStart.turn.id,
      interruptTurnId: interruptStart.turn.id,
      compactTurnId: compactStarted.turn.id,
      compactStatus: compactTerminal.turn.status,
      pendingKillTurnId: pendingTurn.turn.id,
      pendingKillTurnStatus: killedTurn.status,
      hardRestart,
      dynamicToolCalls: [
        ...firstDynamicToolCalls,
        ...resumedClient.dynamicToolCalls,
      ].map((call) => ({
        threadId: call.threadId,
        turnId: call.turnId,
        callId: call.callId,
        namespace: call.namespace,
        tool: call.tool,
        token: call.arguments.token,
      })),
      mcpServers: (mcpStatus.data ?? mcpStatus.items ?? []).map((server) => ({
        name: server.name,
        status: server.status,
        toolCount: server.tools?.length ?? 0,
      })),
      itemTypes: [
        ...new Set([
          ...firstItemTypes,
          ...itemTypes(resumedClient),
          ...itemTypes(recoveryClient),
        ]),
      ].sort(),
      serverRequestMethods: [
        ...new Set([
          ...firstServerRequestMethods,
          ...resumedClient.serverRequests.map((request) => request.method),
          ...recoveryClient.serverRequests.map((request) => request.method),
        ]),
      ].sort(),
      interactionObservations: {
        commandApproval: resumedClient.serverRequests.some(
          (request) =>
            request.method === "item/commandExecution/requestApproval",
        ),
        fileApproval: resumedClient.serverRequests.some(
          (request) => request.method === "item/fileChange/requestApproval",
        ),
        permissionRequest: resumedClient.serverRequests.some(
          (request) => request.method === "item/permissions/requestApproval",
        ),
        requestUserInput: resumedClient.serverRequests.some(
          (request) => request.method === "item/tool/requestUserInput",
        ),
        mcpElicitation: resumedClient.serverRequests.some(
          (request) => request.method === "mcpServer/elicitation/request",
        ),
        subagentItem: itemTypes(resumedClient).some((type) =>
          type.toLowerCase().includes("collab"),
        ),
      },
      unknownServerRequests: [
        ...resumedClient.unknownServerRequests,
        ...recoveryClient.unknownServerRequests,
      ],
      threadTurnCount: threadRead.thread.turns?.length,
      gitDiff: execFileSync("git", ["diff", "--stat"], {
        cwd: scratchRoot,
      })
        .toString()
        .trim(),
    };
    const reportPath = join(scratchRoot, "app-server-spike-report.json");
    writeFileSync(reportPath, `${JSON.stringify(report, null, 2)}\n`);
    chmodSync(reportPath, 0o600);
    console.log(JSON.stringify(report, null, 2));
  } finally {
    await firstClient?.close().catch(() => undefined);
    await resumedClient?.close().catch(() => undefined);
    await recoveryClient?.close().catch(() => undefined);
    if (!keepScratch) rmSync(scratchRoot, { recursive: true, force: true });
  }
}

await main();
