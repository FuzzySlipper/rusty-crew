import assert from "node:assert/strict";
import { createServer, type Server } from "node:http";
import test from "node:test";
import type {
  AgentId,
  BrainEventEnvelope,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import { runBufferedBrainHost } from "../src/buffered-brain-host.js";
import {
  ComfyUiImageGenerationProvider,
  createImageGenerationRuntime,
  imageGenerationConfigFromUnknown,
  imageGenerationTool,
  ImageGenerationError,
  type ImageGenerationProviderConfig,
  type ImageGenerationPreset,
} from "../src/image-generation.js";
import { handleServiceImageGenerationRequest } from "../src/service-image-generation-routes.js";

const imageBytes = Buffer.from("89504e470d0a1a0a", "hex");

function rawConfig(endpointUrl: string) {
  return {
    providers: [
      {
        id: "local-comfy",
        kind: "comfyui",
        endpointUrl,
        bearerTokenEnv: "COMFY_TOKEN",
        requestTimeoutMs: 2_000,
        generationTimeoutMs: 2_000,
        pollIntervalMs: 50,
      },
    ],
    presets: [
      {
        id: "portrait",
        version: "v2",
        providerId: "local-comfy",
        workflow: {
          text: { class_type: "CLIPTextEncode", inputs: { text: "template" } },
          negative: {
            class_type: "CLIPTextEncode",
            inputs: { text: "template-negative" },
          },
          sampler: {
            class_type: "KSampler",
            inputs: { seed: 1, steps: 5 },
          },
          latent: {
            class_type: "EmptyLatentImage",
            inputs: { width: 512, height: 512 },
          },
          output: {
            class_type: "SaveImage",
            inputs: { filename_prefix: "rc" },
          },
        },
        inputs: {
          prompt: { nodeId: "text", inputName: "text" },
          negativePrompt: { nodeId: "negative", inputName: "text" },
          seed: { nodeId: "sampler", inputName: "seed" },
          steps: { nodeId: "sampler", inputName: "steps" },
          width: { nodeId: "latent", inputName: "width" },
          height: { nodeId: "latent", inputName: "height" },
        },
        defaults: {
          negativePrompt: "blurry",
          width: 768,
          height: 768,
          steps: 20,
        },
        limits: {
          minWidth: 256,
          maxWidth: 1024,
          minHeight: 256,
          maxHeight: 1024,
          minSteps: 1,
          maxSteps: 40,
          maxPromptChars: 500,
          maxOutputs: 1,
        },
        styles: {
          ink: { promptSuffix: "ink drawing", negativePromptSuffix: "color" },
        },
        outputNodeIds: ["output"],
      },
    ],
  };
}

async function fakeComfy(
  mode: "success" | "queue-error" | "node-error" | "pending" = "success",
): Promise<{
  endpointUrl: string;
  close(): Promise<void>;
  submitted(): Record<string, unknown> | undefined;
  cancelled(): boolean;
}> {
  let submittedWorkflow: Record<string, unknown> | undefined;
  let historyReads = 0;
  let wasCancelled = false;
  const server = createServer(async (request, response) => {
    const url = new URL(request.url ?? "/", "http://fake-comfy");
    if (request.method === "POST" && url.pathname === "/prompt") {
      assert.equal(request.headers.authorization, "Bearer test-token");
      const chunks: Buffer[] = [];
      for await (const chunk of request) chunks.push(Buffer.from(chunk));
      const body = JSON.parse(Buffer.concat(chunks).toString()) as {
        prompt: Record<string, unknown>;
      };
      submittedWorkflow = body.prompt;
      return json(
        response,
        mode === "queue-error"
          ? { node_errors: { sampler: { errors: ["bad sampler"] } } }
          : { prompt_id: "job-1", node_errors: {} },
      );
    }
    if (request.method === "GET" && url.pathname === "/history/job-1") {
      historyReads += 1;
      if (mode === "node-error") {
        return json(response, {
          "job-1": {
            status: {
              status_str: "error",
              completed: false,
              messages: ["node failed"],
            },
          },
        });
      }
      if (mode === "success" && historyReads > 1) {
        return json(response, {
          "job-1": {
            status: { status_str: "success", completed: true },
            outputs: {
              output: {
                images: [
                  { filename: "result.png", subfolder: "", type: "output" },
                ],
              },
            },
          },
        });
      }
      return json(response, {});
    }
    if (request.method === "GET" && url.pathname === "/queue") {
      return json(response, {
        queue_running: [[0, "job-1"]],
        queue_pending: [],
      });
    }
    if (request.method === "POST" && url.pathname === "/queue") {
      wasCancelled = true;
      return json(response, {});
    }
    if (request.method === "GET" && url.pathname === "/view") {
      response.writeHead(200, { "content-type": "image/png" });
      return response.end(imageBytes);
    }
    response.writeHead(404).end();
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  return {
    endpointUrl: `http://127.0.0.1:${address.port}`,
    close: () => closeServer(server),
    submitted: () => submittedWorkflow,
    cancelled: () => wasCancelled,
  };
}

test("ComfyUI adapter patches an approved workflow and returns typed media with provenance", async () => {
  const fake = await fakeComfy();
  try {
    const config = imageGenerationConfigFromUnknown(
      rawConfig(fake.endpointUrl),
    );
    const runtime = createImageGenerationRuntime(config, {
      env: { COMFY_TOKEN: "test-token" },
    });
    const updates: string[] = [];
    const result = await imageGenerationTool(runtime).execute(
      "call-1",
      {
        preset: "portrait",
        prompt: "a careful engineer",
        seed: 42,
        width: 640,
        height: 512,
        steps: 12,
        style: "ink",
      },
      undefined,
      (update) =>
        updates.push(String((update.details as { status?: string }).status)),
    );
    assert.deepEqual(updates, ["queued", "running", "completed"]);
    assert.deepEqual(result.content, [
      {
        type: "image",
        data: imageBytes.toString("base64"),
        mimeType: "image/png",
      },
    ]);
    const provenance = (
      result.details as { provenance: Record<string, unknown> }
    ).provenance;
    assert.deepEqual(
      {
        provider: provenance.provider_id,
        job: provenance.provider_job_id,
        preset: provenance.workflow_preset_id,
        version: provenance.workflow_preset_version,
        seed: provenance.seed,
        width: provenance.width,
        height: provenance.height,
      },
      {
        provider: "local-comfy",
        job: "job-1",
        preset: "portrait",
        version: "v2",
        seed: 42,
        width: 640,
        height: 512,
      },
    );
    const workflow = fake.submitted() as {
      text: { inputs: { text: string } };
      negative: { inputs: { text: string } };
      sampler: { inputs: { seed: number; steps: number } };
      latent: { inputs: { width: number; height: number } };
    };
    assert.equal(workflow.text.inputs.text, "a careful engineer, ink drawing");
    assert.equal(workflow.negative.inputs.text, "blurry, color");
    assert.deepEqual(workflow.sampler.inputs, { seed: 42, steps: 12 });
    assert.deepEqual(workflow.latent.inputs, { width: 640, height: 512 });
  } finally {
    await fake.close();
  }
});

test("ComfyUI queue and node failures retain stable recoverable reason codes", async () => {
  for (const [mode, reasonCode] of [
    ["queue-error", "comfyui_queue_validation_failed"],
    ["node-error", "comfyui_node_execution_failed"],
  ] as const) {
    const fake = await fakeComfy(mode);
    try {
      const config = imageGenerationConfigFromUnknown(
        rawConfig(fake.endpointUrl),
      );
      const result = await imageGenerationTool(
        createImageGenerationRuntime(config, {
          env: { COMFY_TOKEN: "test-token" },
        }),
      ).execute("call", { preset: "portrait", prompt: "test" });
      assert.equal(
        (result.details as { reasonCode?: string }).reasonCode,
        reasonCode,
      );
      assert.equal((result.details as { ok?: boolean }).ok, false);
    } finally {
      await fake.close();
    }
  }
});

test("ComfyUI cancellation removes the queued job and reports cancellation", async () => {
  const fake = await fakeComfy("pending");
  try {
    const config = imageGenerationConfigFromUnknown(
      rawConfig(fake.endpointUrl),
    );
    const controller = new AbortController();
    const statuses: string[] = [];
    const promise = imageGenerationTool(
      createImageGenerationRuntime(config, {
        env: { COMFY_TOKEN: "test-token" },
      }),
    ).execute(
      "call",
      { preset: "portrait", prompt: "test" },
      controller.signal,
      (update) => {
        const status = String((update.details as { status?: string }).status);
        statuses.push(status);
        if (status === "queued") controller.abort();
      },
    );
    const result = await promise;
    assert.equal(
      (result.details as { reasonCode?: string }).reasonCode,
      "image_generation_cancelled",
    );
    assert.equal(fake.cancelled(), true);
    assert.deepEqual(statuses, ["queued", "cancelled"]);
  } finally {
    await fake.close();
  }
});

test("image generation configuration rejects provider, preset, and limit escape hatches", () => {
  const base = rawConfig("http://127.0.0.1:8188");
  assert.throws(
    () =>
      imageGenerationConfigFromUnknown({
        ...base,
        providers: [
          { id: "x", kind: "arbitrary", endpointUrl: "http://127.0.0.1" },
        ],
      }),
    /unsupported image generation provider kind/,
  );
  assert.throws(
    () =>
      imageGenerationConfigFromUnknown({
        ...base,
        presets: [{ ...base.presets[0], providerId: "missing" }],
      }),
    /references unknown provider/,
  );
  assert.throws(
    () =>
      imageGenerationConfigFromUnknown({
        ...base,
        presets: [
          {
            ...base.presets[0],
            defaults: { ...base.presets[0].defaults, width: 2048 },
          },
        ],
      }),
    /default width must be within/,
  );
});

test("ComfyUI provider exposes unreachable and timeout failures explicitly", async () => {
  const providerConfig: ImageGenerationProviderConfig = {
    id: "unreachable",
    kind: "comfyui",
    endpointUrl: "http://127.0.0.1:1/",
    requestTimeoutMs: 100,
    generationTimeoutMs: 100,
    pollIntervalMs: 10,
    allowGlobalInterrupt: false,
  };
  const preset = imageGenerationConfigFromUnknown(
    rawConfig("http://127.0.0.1:8188"),
  ).presets[0] as ImageGenerationPreset;
  const provider = new ComfyUiImageGenerationProvider(
    providerConfig,
    async () => {
      throw new TypeError("connection refused");
    },
  );
  await assert.rejects(
    provider.generate(
      preset,
      {
        presetId: preset.id,
        prompt: "test",
        seed: 1,
        width: 512,
        height: 512,
        steps: 10,
      },
      {},
    ),
    (error: unknown) =>
      error instanceof ImageGenerationError &&
      error.reasonCode === "comfyui_unreachable",
  );

  const fake = await fakeComfy("pending");
  try {
    const timeoutProvider = new ComfyUiImageGenerationProvider(
      {
        ...providerConfig,
        id: "timeout",
        endpointUrl: fake.endpointUrl,
        bearerTokenEnv: "COMFY_TOKEN",
        requestTimeoutMs: 500,
        generationTimeoutMs: 60,
      },
      fetch,
      { COMFY_TOKEN: "test-token" },
    );
    await assert.rejects(
      timeoutProvider.generate(
        preset,
        {
          presetId: preset.id,
          prompt: "test",
          seed: 1,
          width: 512,
          height: 512,
          steps: 10,
        },
        {},
      ),
      (error: unknown) =>
        error instanceof ImageGenerationError &&
        error.reasonCode === "image_generation_timeout",
    );
    assert.equal(fake.cancelled(), true);
  } finally {
    await fake.close();
  }
});

test("operator routes expose a redacted preset catalog and persist generated media", async () => {
  const config = imageGenerationConfigFromUnknown(
    rawConfig("http://secret-comfy.internal:8188"),
  );
  const preset = config.presets[0]!;
  const runtime = {
    config,
    providers: new Map([
      [
        "local-comfy",
        {
          id: "local-comfy",
          kind: "fake",
          async generate() {
            return {
              status: "completed" as const,
              providerId: "local-comfy",
              providerKind: "fake",
              jobId: "operator-job",
              presetId: preset.id,
              presetVersion: preset.version,
              request: {
                presetId: preset.id,
                prompt: "operator prompt",
                seed: 7,
                width: 768,
                height: 768,
                steps: 20,
              },
              images: [
                { data: imageBytes.toString("base64"), mimeType: "image/png" },
              ],
              statusHistory: [],
            };
          },
        },
      ],
    ]),
  };
  const persisted: unknown[] = [];
  const context = {
    runtime: () => runtime,
    listSessions: async () => [{ sessionId: "session-1" }] as never,
    toolMediaAttachments: {
      async persistImages(input: unknown) {
        persisted.push(input);
        return [{ attachmentId: "attachment-1", mimeType: "image/png" }];
      },
    } as never,
  };
  const catalog = await handleServiceImageGenerationRequest(
    {
      method: "GET",
      url: new URL("http://service/v1/admin/image-generation/presets"),
      requestId: "catalog-request",
    },
    context,
  );
  assert.ok("status" in catalog);
  assert.equal(catalog.status, 200);
  assert.equal(JSON.stringify(catalog).includes("secret-comfy"), false);
  assert.equal(JSON.stringify(catalog).includes("workflow"), false);
  assert.equal(JSON.stringify(catalog).includes("COMFY_TOKEN"), false);

  const generated = await handleServiceImageGenerationRequest(
    {
      method: "POST",
      url: new URL("http://service/v1/admin/image-generation/generate"),
      requestId: "generate-request",
      body: {
        session_id: "session-1",
        preset: "portrait",
        prompt: "operator prompt",
        seed: 7,
      },
    },
    context,
  );
  assert.ok("status" in generated);
  assert.equal(generated.status, 200);
  assert.equal(persisted.length, 1);
  assert.equal(
    JSON.stringify(generated).includes(imageBytes.toString("base64")),
    false,
  );
  assert.match(JSON.stringify(generated), /attachment-1/);
});

test("buffered brain host projects image job status into durable provider events", async () => {
  const config = imageGenerationConfigFromUnknown(
    rawConfig("http://unused.invalid"),
  );
  const preset = config.presets[0]!;
  const runtime = {
    config,
    providers: new Map([
      [
        "local-comfy",
        {
          id: "local-comfy",
          kind: "fake",
          async generate(
            _preset: ImageGenerationPreset,
            request: import("../src/image-generation.js").ImageGenerationRequest,
            options: {
              onStatus?(
                update: import("../src/image-generation.js").ImageGenerationStatusUpdate,
              ): void;
            },
          ) {
            for (const status of ["queued", "running", "completed"] as const) {
              options.onStatus?.({
                status,
                providerId: "local-comfy",
                jobId: "status-job",
                message: `image ${status}`,
              });
            }
            return {
              status: "completed" as const,
              providerId: "local-comfy",
              providerKind: "fake",
              jobId: "status-job",
              presetId: preset.id,
              presetVersion: preset.version,
              request,
              images: [
                { data: imageBytes.toString("base64"), mimeType: "image/png" },
              ],
              statusHistory: [],
            };
          },
        },
      ],
    ]),
  };
  let drainCount = 0;
  const submitted: unknown[] = [];
  const bridge = {
    startBrainRun: async () => ({
      moduleId: "openai-responses" as const,
      wakeId: "image-wake",
    }),
    drainBrainRun: async () => {
      drainCount += 1;
      return drainCount === 1
        ? {
            moduleId: "openai-responses" as const,
            wakeId: "image-wake",
            items: [],
            toolRequests: [
              {
                wakeId: "image-wake",
                callId: "image-call",
                name: "image_generate",
                argumentsJson: JSON.stringify({
                  preset: "portrait",
                  prompt: "status projection",
                }),
              },
            ],
            terminal: false,
          }
        : {
            moduleId: "openai-responses" as const,
            wakeId: "image-wake",
            items: [],
            toolRequests: [],
            terminal: true,
          };
    },
    submitBrainHostResult: async (input: unknown) => {
      submitted.push(input);
      return {
        moduleId: "openai-responses",
        wakeId: "image-wake",
        callId: "image-call",
      };
    },
  } as unknown as NativeBridgeModule;
  const events: BrainEventEnvelope[] = [];
  const sessionId = "image-session" as SessionId;
  const wake = {
    wakeId: "image-wake",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "instructions" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId: "image-agent" as AgentId,
        profileId: "image-profile" as ProfileId,
        kind: "full" as const,
        resourceLimits: {},
        toolProfile: {
          tools: [{ name: "image_generate", description: "Generate" }],
        },
        status: "idle" as const,
        brainTurnCount: 0,
        createdAt: "2026-07-25T00:00:00Z",
        lastActiveAt: "2026-07-25T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: {
        mode: "frozen_snapshot_next_wake" as const,
        queueOwner: "body" as const,
        queuedMessageTtlMs: 5_000,
        maxQueuedMessages: 32,
      },
    },
  };
  await runBufferedBrainHost({
    bridge,
    moduleLabel: "OpenAI Responses",
    run: {
      moduleId: "openai-responses",
      providerInput: {
        wakeId: wake.wakeId,
        sessionId,
        bodyState: wake.state,
        config: { model: "test" },
        client: { mode: "fake" },
      },
    },
    wake,
    toolProfile: wake.state.session.toolProfile,
    toolResolver: () => [imageGenerationTool(runtime)],
    submitEvent: async (event) => {
      events.push(event);
    },
  });
  assert.equal(submitted.length, 1);
  assert.deepEqual(
    events.map(
      (event) =>
        JSON.parse((event.event as { metadataJson: string }).metadataJson)
          .status,
    ),
    ["queued", "running", "completed"],
  );
});

function json(
  response: import("node:http").ServerResponse,
  value: unknown,
): void {
  response.writeHead(200, { "content-type": "application/json" });
  response.end(JSON.stringify(value));
}

async function closeServer(server: Server): Promise<void> {
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
}
