import { randomInt, randomUUID } from "node:crypto";
import { Type, type TSchema } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export type ImageGenerationStatus =
  | "queued"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export interface ImageGenerationRequest {
  presetId: string;
  prompt: string;
  negativePrompt?: string;
  seed: number;
  width: number;
  height: number;
  steps: number;
  style?: string;
}

export interface ImageGenerationStatusUpdate {
  status: ImageGenerationStatus;
  providerId: string;
  jobId?: string;
  message: string;
}

export interface ImageGenerationOutput {
  data: string;
  mimeType: string;
}

export interface ImageGenerationResult {
  status: "completed";
  providerId: string;
  providerKind: string;
  jobId: string;
  presetId: string;
  presetVersion: string;
  request: ImageGenerationRequest;
  images: ImageGenerationOutput[];
  statusHistory: ImageGenerationStatusUpdate[];
}

export interface ImageGenerationProvider {
  readonly id: string;
  readonly kind: string;
  generate(
    preset: ImageGenerationPreset,
    request: ImageGenerationRequest,
    options: {
      signal?: AbortSignal;
      onStatus?(update: ImageGenerationStatusUpdate): void;
    },
  ): Promise<ImageGenerationResult>;
}

export interface ImageGenerationInputBinding {
  nodeId: string;
  inputName: string;
}

export interface ImageGenerationStyle {
  promptSuffix?: string;
  negativePromptSuffix?: string;
}

export interface ImageGenerationPreset {
  id: string;
  version: string;
  providerId: string;
  workflow: Record<string, unknown>;
  inputs: {
    prompt: ImageGenerationInputBinding;
    negativePrompt?: ImageGenerationInputBinding;
    seed?: ImageGenerationInputBinding;
    width?: ImageGenerationInputBinding;
    height?: ImageGenerationInputBinding;
    steps?: ImageGenerationInputBinding;
  };
  defaults: {
    negativePrompt?: string;
    width: number;
    height: number;
    steps: number;
  };
  limits: {
    minWidth: number;
    maxWidth: number;
    minHeight: number;
    maxHeight: number;
    minSteps: number;
    maxSteps: number;
    maxPromptChars: number;
    maxOutputs: number;
  };
  styles: Record<string, ImageGenerationStyle>;
  outputNodeIds: string[];
}

export interface ComfyUiImageGenerationProviderConfig {
  id: string;
  kind: "comfyui";
  endpointUrl: string;
  bearerTokenEnv?: string;
  requestTimeoutMs: number;
  generationTimeoutMs: number;
  pollIntervalMs: number;
  allowGlobalInterrupt: boolean;
}

export type ImageGenerationProviderConfig =
  ComfyUiImageGenerationProviderConfig;

export interface ImageGenerationConfig {
  providers: ImageGenerationProviderConfig[];
  presets: ImageGenerationPreset[];
}

export interface ImageGenerationRuntime {
  config: ImageGenerationConfig;
  providers: ReadonlyMap<string, ImageGenerationProvider>;
}

const imageGenerateParameters = Type.Object({
  preset: Type.String({ minLength: 1, maxLength: 128 }),
  prompt: Type.String({ minLength: 1, maxLength: 16_000 }),
  negative_prompt: Type.Optional(Type.String({ maxLength: 16_000 })),
  seed: Type.Optional(Type.Integer({ minimum: 0, maximum: 4_294_967_295 })),
  width: Type.Optional(Type.Integer({ minimum: 64, maximum: 16_384 })),
  height: Type.Optional(Type.Integer({ minimum: 64, maximum: 16_384 })),
  steps: Type.Optional(Type.Integer({ minimum: 1, maximum: 1_000 })),
  style: Type.Optional(Type.String({ minLength: 1, maxLength: 128 })),
});

interface ImageGenerateArguments {
  preset: string;
  prompt: string;
  negative_prompt?: string;
  seed?: number;
  width?: number;
  height?: number;
  steps?: number;
  style?: string;
}

export function imageGenerationConfigFromUnknown(
  value: unknown,
): ImageGenerationConfig {
  if (value === undefined || value === null) {
    return { providers: [], presets: [] };
  }
  const root = requiredRecord(value, "imageGeneration");
  const providers = requiredArray(
    root.providers,
    "imageGeneration.providers",
  ).map(configuredProvider);
  const providerIds = new Set<string>();
  for (const provider of providers) {
    if (providerIds.has(provider.id)) {
      throw new Error(`duplicate image generation provider ${provider.id}`);
    }
    providerIds.add(provider.id);
  }
  const presets = requiredArray(root.presets, "imageGeneration.presets").map(
    configuredPreset,
  );
  const presetIds = new Set<string>();
  for (const preset of presets) {
    if (presetIds.has(preset.id)) {
      throw new Error(`duplicate image generation preset ${preset.id}`);
    }
    if (!providerIds.has(preset.providerId)) {
      throw new Error(
        `image generation preset ${preset.id} references unknown provider ${preset.providerId}`,
      );
    }
    presetIds.add(preset.id);
  }
  return { providers, presets };
}

export function createImageGenerationToolResolver(
  runtime: ImageGenerationRuntime | undefined,
): BrainToolResolver {
  if (!runtime || runtime.config.presets.length === 0) return () => [];
  return () => [imageGenerationTool(runtime)];
}

export function imageGenerationTool(
  runtime: ImageGenerationRuntime,
): BrainTool<TSchema, Record<string, unknown>> {
  const presets = new Map(
    runtime.config.presets.map((preset) => [preset.id, preset]),
  );
  return {
    name: "image_generate",
    label: "Generate image",
    description: `Generate an image with one approved server-side workflow preset. Available presets: ${runtime.config.presets
      .map((preset) => preset.id)
      .join(", ")}.`,
    parameters: imageGenerateParameters,
    prepareArguments: (value) =>
      prepareImageGenerationArguments(value, presets) as never,
    executionMode: "sequential",
    execute: async (_callId, params, signal, onUpdate) =>
      executeImageGeneration(
        runtime,
        presets,
        params as unknown as ImageGenerateArguments,
        signal,
        onUpdate,
      ),
    executeWithContext: async (params, context) =>
      executeImageGeneration(
        runtime,
        presets,
        params as unknown as ImageGenerateArguments,
        context.signal,
        context.onUpdate,
      ),
  };
}

async function executeImageGeneration(
  runtime: ImageGenerationRuntime,
  presets: ReadonlyMap<string, ImageGenerationPreset>,
  params: ImageGenerateArguments,
  signal: AbortSignal | undefined,
  onUpdate:
    | ((partial: BrainToolResult<Record<string, unknown>>) => void)
    | undefined,
): Promise<BrainToolResult<Record<string, unknown>>> {
  const preset = presets.get(params.preset);
  if (!preset) {
    return failedToolResult(
      "image_generation_preset_not_found",
      `image generation preset ${params.preset} is not configured`,
      false,
    );
  }
  const provider = runtime.providers.get(preset.providerId);
  if (!provider) {
    return failedToolResult(
      "image_generation_provider_unavailable",
      `image generation provider ${preset.providerId} is not available`,
      true,
    );
  }
  const request = normalizedImageGenerationRequest(params, preset);
  try {
    const result = await provider.generate(preset, request, {
      signal,
      onStatus(update) {
        onUpdate?.({
          content: [{ type: "text", text: update.message }],
          details: {
            ok: true,
            source: "image_generation_status",
            status: update.status,
            providerId: update.providerId,
            jobId: update.jobId,
          },
        });
      },
    });
    return {
      content: result.images.map((image) => ({
        type: "image" as const,
        data: image.data,
        mimeType: image.mimeType,
      })),
      details: {
        ok: true,
        status: "completed",
        imageCount: result.images.length,
        statusHistory: result.statusHistory,
        provenance: {
          adapter: result.providerKind,
          provider_id: result.providerId,
          provider_job_id: result.jobId,
          workflow_preset_id: result.presetId,
          workflow_preset_version: result.presetVersion,
          prompt: result.request.prompt,
          negative_prompt: result.request.negativePrompt,
          seed: result.request.seed,
          width: result.request.width,
          height: result.request.height,
          steps: result.request.steps,
          style: result.request.style,
        },
      },
    };
  } catch (error) {
    const failure = imageGenerationFailure(error);
    return failedToolResult(
      failure.reasonCode,
      failure.message,
      failure.retryable,
    );
  }
}

export class ComfyUiImageGenerationProvider implements ImageGenerationProvider {
  readonly kind = "comfyui";
  readonly id: string;

  constructor(
    private readonly config: ComfyUiImageGenerationProviderConfig,
    private readonly fetchImpl: typeof fetch = fetch,
    private readonly env: Readonly<
      Record<string, string | undefined>
    > = process.env,
  ) {
    this.id = config.id;
  }

  async generate(
    preset: ImageGenerationPreset,
    request: ImageGenerationRequest,
    options: {
      signal?: AbortSignal;
      onStatus?(update: ImageGenerationStatusUpdate): void;
    },
  ): Promise<ImageGenerationResult> {
    const workflow = applyWorkflowInputs(preset, request);
    const clientId = `rusty-crew-${randomUUID()}`;
    const queueResponse = await this.requestJson("prompt", {
      method: "POST",
      body: JSON.stringify({ prompt: workflow, client_id: clientId }),
      signal: options.signal,
    });
    const nodeErrors = recordValue(queueResponse.node_errors);
    if (Object.keys(nodeErrors).length > 0) {
      throw new ImageGenerationError(
        "comfyui_queue_validation_failed",
        `ComfyUI rejected workflow preset ${preset.id}: ${JSON.stringify(nodeErrors)}`,
        false,
      );
    }
    const jobId = requiredString(queueResponse.prompt_id, "ComfyUI prompt_id");
    const statusHistory: ImageGenerationStatusUpdate[] = [];
    const report = (status: ImageGenerationStatus, message: string) => {
      const update = { status, providerId: this.id, jobId, message };
      if (statusHistory.at(-1)?.status === status) return;
      statusHistory.push(update);
      options.onStatus?.(update);
    };
    report("queued", `Image generation ${jobId} queued.`);
    const deadline = Date.now() + this.config.generationTimeoutMs;
    try {
      for (;;) {
        if (options.signal?.aborted) {
          await this.cancel(jobId).catch(() => undefined);
          report("cancelled", `Image generation ${jobId} cancelled.`);
          throw new ImageGenerationError(
            "image_generation_cancelled",
            `image generation ${jobId} was cancelled`,
            false,
          );
        }
        if (Date.now() >= deadline) {
          await this.cancel(jobId).catch(() => undefined);
          throw new ImageGenerationError(
            "image_generation_timeout",
            `image generation ${jobId} exceeded ${this.config.generationTimeoutMs}ms`,
            true,
          );
        }
        const history = await this.requestJson(
          `history/${encodeURIComponent(jobId)}`,
          { method: "GET", signal: options.signal },
        );
        const entry = recordValue(history[jobId]);
        if (Object.keys(entry).length > 0) {
          const status = recordValue(entry.status);
          if (status.status_str === "error") {
            throw new ImageGenerationError(
              "comfyui_node_execution_failed",
              `ComfyUI job ${jobId} failed: ${safeJson(status.messages ?? status)}`,
              true,
            );
          }
          const images = imageDescriptors(
            entry.outputs,
            preset.outputNodeIds,
          ).slice(0, preset.limits.maxOutputs);
          if (images.length > 0) {
            report("running", `Image generation ${jobId} produced output.`);
            const outputs = await Promise.all(
              images.map((image) => this.readImage(image, options.signal)),
            );
            report("completed", `Image generation ${jobId} completed.`);
            return {
              status: "completed",
              providerId: this.id,
              providerKind: this.kind,
              jobId,
              presetId: preset.id,
              presetVersion: preset.version,
              request,
              images: outputs,
              statusHistory,
            };
          }
        }
        const queue = await this.requestJson("queue", {
          method: "GET",
          signal: options.signal,
        });
        if (queueContainsJob(queue.queue_running, jobId)) {
          report("running", `Image generation ${jobId} is running.`);
        }
        await abortableDelay(this.config.pollIntervalMs, options.signal);
      }
    } catch (error) {
      if (
        options.signal?.aborted &&
        !(
          error instanceof ImageGenerationError &&
          error.reasonCode === "image_generation_cancelled"
        )
      ) {
        await this.cancel(jobId).catch(() => undefined);
        report("cancelled", `Image generation ${jobId} cancelled.`);
        throw new ImageGenerationError(
          "image_generation_cancelled",
          `image generation ${jobId} was cancelled`,
          false,
        );
      }
      if (
        !(error instanceof ImageGenerationError) ||
        error.reasonCode !== "image_generation_cancelled"
      ) {
        report("failed", `Image generation ${jobId} failed.`);
      }
      throw error;
    }
  }

  private async cancel(jobId: string): Promise<void> {
    await this.requestJson("queue", {
      method: "POST",
      body: JSON.stringify({ delete: [jobId] }),
    });
    if (this.config.allowGlobalInterrupt) {
      await this.requestJson("interrupt", {
        method: "POST",
        body: "{}",
      });
    }
  }

  private async readImage(
    image: ComfyImageDescriptor,
    signal: AbortSignal | undefined,
  ): Promise<ImageGenerationOutput> {
    const url = new URL("view", withTrailingSlash(this.config.endpointUrl));
    url.searchParams.set("filename", image.filename);
    url.searchParams.set("subfolder", image.subfolder);
    url.searchParams.set("type", image.type);
    const response = await this.fetchImpl(url, {
      method: "GET",
      headers: this.headers(false),
      signal: timeoutSignal(signal, this.config.requestTimeoutMs),
    });
    if (!response.ok) {
      throw new ImageGenerationError(
        "comfyui_image_read_failed",
        `ComfyUI image read returned ${response.status}`,
        response.status >= 500,
      );
    }
    const mimeType = response.headers
      .get("content-type")
      ?.split(";")[0]
      ?.trim();
    if (
      !mimeType ||
      !["image/png", "image/jpeg", "image/gif", "image/webp"].includes(mimeType)
    ) {
      throw new ImageGenerationError(
        "comfyui_invalid_image_mime_type",
        `ComfyUI returned unsupported image MIME type ${mimeType ?? "missing"}`,
        false,
      );
    }
    return {
      data: Buffer.from(await response.arrayBuffer()).toString("base64"),
      mimeType,
    };
  }

  private async requestJson(
    path: string,
    init: RequestInit,
  ): Promise<Record<string, unknown>> {
    const url = new URL(path, withTrailingSlash(this.config.endpointUrl));
    let response: Response;
    try {
      response = await this.fetchImpl(url, {
        ...init,
        headers: this.headers(init.body !== undefined),
        signal: timeoutSignal(init.signal, this.config.requestTimeoutMs),
      });
    } catch (error) {
      if (init.signal?.aborted) throw error;
      if (error instanceof DOMException && error.name === "TimeoutError") {
        throw new ImageGenerationError(
          "comfyui_request_timeout",
          `ComfyUI ${path} request exceeded ${this.config.requestTimeoutMs}ms`,
          true,
        );
      }
      throw new ImageGenerationError(
        "comfyui_unreachable",
        `ComfyUI request failed: ${error instanceof Error ? error.message : String(error)}`,
        true,
      );
    }
    if (!response.ok) {
      throw new ImageGenerationError(
        "comfyui_http_error",
        `ComfyUI ${path} returned ${response.status}: ${(await response.text()).slice(0, 1_000)}`,
        response.status >= 500 || response.status === 429,
      );
    }
    const value: unknown = await response.json();
    return requiredRecord(value, `ComfyUI ${path} response`);
  }

  private headers(json: boolean): Record<string, string> {
    const headers: Record<string, string> = json
      ? { "content-type": "application/json" }
      : {};
    if (this.config.bearerTokenEnv) {
      const token = this.env[this.config.bearerTokenEnv];
      if (!token) {
        throw new ImageGenerationError(
          "image_generation_credential_missing",
          `image generation credential env ${this.config.bearerTokenEnv} is missing`,
          false,
        );
      }
      headers.authorization = `Bearer ${token}`;
    }
    return headers;
  }
}

export class ImageGenerationError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
    readonly retryable: boolean,
  ) {
    super(message);
    this.name = "ImageGenerationError";
  }
}

export function createImageGenerationRuntime(
  config: ImageGenerationConfig,
  options: {
    fetchImpl?: typeof fetch;
    env?: Readonly<Record<string, string | undefined>>;
  } = {},
): ImageGenerationRuntime {
  return {
    config,
    providers: new Map(
      config.providers.map((provider) => [
        provider.id,
        new ComfyUiImageGenerationProvider(
          provider,
          options.fetchImpl,
          options.env,
        ),
      ]),
    ),
  };
}

function configuredProvider(
  value: unknown,
  index: number,
): ImageGenerationProviderConfig {
  const record = requiredRecord(value, `imageGeneration.providers[${index}]`);
  const kind = requiredString(record.kind, "image generation provider kind");
  if (kind !== "comfyui") {
    throw new Error(`unsupported image generation provider kind ${kind}`);
  }
  const endpointUrl = requiredHttpUrl(
    record.endpointUrl,
    `imageGeneration.providers[${index}].endpointUrl`,
  );
  return {
    id: stableName(record.id, `imageGeneration.providers[${index}].id`),
    kind,
    endpointUrl,
    bearerTokenEnv: optionalString(record.bearerTokenEnv),
    requestTimeoutMs: boundedInteger(
      record.requestTimeoutMs,
      30_000,
      100,
      300_000,
    ),
    generationTimeoutMs: boundedInteger(
      record.generationTimeoutMs,
      300_000,
      1_000,
      3_600_000,
    ),
    pollIntervalMs: boundedInteger(record.pollIntervalMs, 500, 50, 30_000),
    allowGlobalInterrupt: record.allowGlobalInterrupt === true,
  };
}

function configuredPreset(
  value: unknown,
  index: number,
): ImageGenerationPreset {
  const record = requiredRecord(value, `imageGeneration.presets[${index}]`);
  const defaults = recordValue(record.defaults);
  const limits = recordValue(record.limits);
  const inputs = requiredRecord(
    record.inputs,
    `imageGeneration.presets[${index}].inputs`,
  );
  const styles = Object.fromEntries(
    Object.entries(recordValue(record.styles)).map(([name, style]) => {
      stableName(name, `image generation style ${name}`);
      const styleRecord = requiredRecord(
        style,
        `image generation style ${name}`,
      );
      return [
        name,
        {
          promptSuffix: optionalString(styleRecord.promptSuffix),
          negativePromptSuffix: optionalString(
            styleRecord.negativePromptSuffix,
          ),
        },
      ];
    }),
  );
  const preset: ImageGenerationPreset = {
    id: stableName(record.id, `imageGeneration.presets[${index}].id`),
    version: requiredString(record.version, "image generation preset version"),
    providerId: stableName(
      record.providerId,
      `imageGeneration.presets[${index}].providerId`,
    ),
    workflow: requiredRecord(
      record.workflow,
      `imageGeneration.presets[${index}].workflow`,
    ),
    inputs: {
      prompt: inputBinding(inputs.prompt, "prompt"),
      negativePrompt: optionalInputBinding(
        inputs.negativePrompt,
        "negativePrompt",
      ),
      seed: inputBinding(inputs.seed, "seed"),
      width: inputBinding(inputs.width, "width"),
      height: inputBinding(inputs.height, "height"),
      steps: inputBinding(inputs.steps, "steps"),
    },
    defaults: {
      negativePrompt: optionalString(defaults.negativePrompt),
      width: boundedInteger(defaults.width, 1024, 64, 16_384),
      height: boundedInteger(defaults.height, 1024, 64, 16_384),
      steps: boundedInteger(defaults.steps, 20, 1, 1_000),
    },
    limits: {
      minWidth: boundedInteger(limits.minWidth, 64, 64, 16_384),
      maxWidth: boundedInteger(limits.maxWidth, 2_048, 64, 16_384),
      minHeight: boundedInteger(limits.minHeight, 64, 64, 16_384),
      maxHeight: boundedInteger(limits.maxHeight, 2_048, 64, 16_384),
      minSteps: boundedInteger(limits.minSteps, 1, 1, 1_000),
      maxSteps: boundedInteger(limits.maxSteps, 100, 1, 1_000),
      maxPromptChars: boundedInteger(limits.maxPromptChars, 8_000, 1, 16_000),
      maxOutputs: boundedInteger(limits.maxOutputs, 1, 1, 4),
    },
    styles,
    outputNodeIds: optionalStringArray(record.outputNodeIds),
  };
  if (
    preset.limits.minWidth > preset.limits.maxWidth ||
    preset.limits.minHeight > preset.limits.maxHeight ||
    preset.limits.minSteps > preset.limits.maxSteps
  ) {
    throw new Error(`image generation preset ${preset.id} has inverted limits`);
  }
  assertRange(
    preset.defaults.width,
    preset.limits.minWidth,
    preset.limits.maxWidth,
    `${preset.id} default width`,
  );
  assertRange(
    preset.defaults.height,
    preset.limits.minHeight,
    preset.limits.maxHeight,
    `${preset.id} default height`,
  );
  assertRange(
    preset.defaults.steps,
    preset.limits.minSteps,
    preset.limits.maxSteps,
    `${preset.id} default steps`,
  );
  return preset;
}

function prepareImageGenerationArguments(
  value: unknown,
  presets: ReadonlyMap<string, ImageGenerationPreset>,
): ImageGenerateArguments {
  const record = requiredRecord(value, "image_generate arguments");
  const preset = requiredString(record.preset, "image_generate preset");
  if (!presets.has(preset)) {
    throw new Error(`image generation preset ${preset} is not configured`);
  }
  return {
    preset,
    prompt: requiredString(record.prompt, "image_generate prompt"),
    negative_prompt: optionalString(record.negative_prompt),
    seed: optionalInteger(record.seed, "image_generate seed"),
    width: optionalInteger(record.width, "image_generate width"),
    height: optionalInteger(record.height, "image_generate height"),
    steps: optionalInteger(record.steps, "image_generate steps"),
    style: optionalString(record.style),
  };
}

function normalizedImageGenerationRequest(
  input: ImageGenerateArguments,
  preset: ImageGenerationPreset,
): ImageGenerationRequest {
  const style = input.style ? preset.styles[input.style] : undefined;
  if (input.style && !style) {
    throw new Error(
      `image generation style ${input.style} is not approved for preset ${preset.id}`,
    );
  }
  const prompt = appendSuffix(input.prompt.trim(), style?.promptSuffix);
  const negativePrompt = appendSuffix(
    input.negative_prompt ?? preset.defaults.negativePrompt ?? "",
    style?.negativePromptSuffix,
  );
  if (prompt.length > preset.limits.maxPromptChars) {
    throw new Error(
      `image generation prompt exceeds ${preset.limits.maxPromptChars} characters`,
    );
  }
  const width = input.width ?? preset.defaults.width;
  const height = input.height ?? preset.defaults.height;
  const steps = input.steps ?? preset.defaults.steps;
  assertRange(width, preset.limits.minWidth, preset.limits.maxWidth, "width");
  assertRange(
    height,
    preset.limits.minHeight,
    preset.limits.maxHeight,
    "height",
  );
  assertRange(steps, preset.limits.minSteps, preset.limits.maxSteps, "steps");
  return {
    presetId: preset.id,
    prompt,
    ...(negativePrompt ? { negativePrompt } : {}),
    seed: input.seed ?? randomInt(0, 4_294_967_296),
    width,
    height,
    steps,
    ...(input.style ? { style: input.style } : {}),
  };
}

function applyWorkflowInputs(
  preset: ImageGenerationPreset,
  request: ImageGenerationRequest,
): Record<string, unknown> {
  const workflow = structuredClone(preset.workflow);
  writeWorkflowInput(workflow, preset.inputs.prompt, request.prompt);
  writeOptionalWorkflowInput(
    workflow,
    preset.inputs.negativePrompt,
    request.negativePrompt ?? "",
  );
  writeOptionalWorkflowInput(workflow, preset.inputs.seed, request.seed);
  writeOptionalWorkflowInput(workflow, preset.inputs.width, request.width);
  writeOptionalWorkflowInput(workflow, preset.inputs.height, request.height);
  writeOptionalWorkflowInput(workflow, preset.inputs.steps, request.steps);
  return workflow;
}

function writeOptionalWorkflowInput(
  workflow: Record<string, unknown>,
  binding: ImageGenerationInputBinding | undefined,
  value: unknown,
): void {
  if (binding) writeWorkflowInput(workflow, binding, value);
}

function writeWorkflowInput(
  workflow: Record<string, unknown>,
  binding: ImageGenerationInputBinding,
  value: unknown,
): void {
  const node = requiredRecord(
    workflow[binding.nodeId],
    `workflow node ${binding.nodeId}`,
  );
  const inputs = requiredRecord(
    node.inputs,
    `workflow node ${binding.nodeId}.inputs`,
  );
  if (!(binding.inputName in inputs)) {
    throw new ImageGenerationError(
      "image_generation_workflow_binding_invalid",
      `workflow node ${binding.nodeId} has no input ${binding.inputName}`,
      false,
    );
  }
  inputs[binding.inputName] = value;
}

interface ComfyImageDescriptor {
  filename: string;
  subfolder: string;
  type: string;
}

function imageDescriptors(
  outputs: unknown,
  outputNodeIds: readonly string[],
): ComfyImageDescriptor[] {
  const outputRecord = recordValue(outputs);
  const allowed = new Set(outputNodeIds);
  const descriptors: ComfyImageDescriptor[] = [];
  for (const [nodeId, rawOutput] of Object.entries(outputRecord)) {
    if (allowed.size > 0 && !allowed.has(nodeId)) continue;
    const images = recordValue(rawOutput).images;
    if (!Array.isArray(images)) continue;
    for (const image of images) {
      const value = requiredRecord(image, `ComfyUI output image ${nodeId}`);
      descriptors.push({
        filename: safeComfyPathPart(value.filename, "filename"),
        subfolder: safeComfyPathPart(value.subfolder ?? "", "subfolder", true),
        type: safeComfyPathPart(value.type ?? "output", "type"),
      });
    }
  }
  return descriptors;
}

function queueContainsJob(value: unknown, jobId: string): boolean {
  return (
    Array.isArray(value) &&
    value.some(
      (item) => Array.isArray(item) && item.some((part) => part === jobId),
    )
  );
}

function inputBinding(
  value: unknown,
  label: string,
): ImageGenerationInputBinding {
  const record = requiredRecord(
    value,
    `image generation ${label} input binding`,
  );
  return {
    nodeId: stableName(record.nodeId, `${label}.nodeId`),
    inputName: stableName(record.inputName, `${label}.inputName`),
  };
}

function optionalInputBinding(
  value: unknown,
  label: string,
): ImageGenerationInputBinding | undefined {
  return value === undefined || value === null
    ? undefined
    : inputBinding(value, label);
}

function failedToolResult(
  reasonCode: string,
  message: string,
  retryable: boolean,
): BrainToolResult<Record<string, unknown>> {
  return {
    content: [{ type: "text", text: message }],
    details: {
      ok: false,
      action: "failed",
      reasonCode,
      message,
      retryable,
    },
  };
}

function imageGenerationFailure(error: unknown): {
  reasonCode: string;
  message: string;
  retryable: boolean;
} {
  if (error instanceof ImageGenerationError) {
    return {
      reasonCode: error.reasonCode,
      message: error.message,
      retryable: error.retryable,
    };
  }
  return {
    reasonCode: "image_generation_failed",
    message: error instanceof Error ? error.message : String(error),
    retryable: false,
  };
}

function timeoutSignal(
  upstream: AbortSignal | null | undefined,
  timeoutMs: number,
): AbortSignal {
  return upstream
    ? AbortSignal.any([upstream, AbortSignal.timeout(timeoutMs)])
    : AbortSignal.timeout(timeoutMs);
}

async function abortableDelay(
  durationMs: number,
  signal: AbortSignal | undefined,
): Promise<void> {
  if (signal?.aborted) throw signal.reason;
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(resolve, durationMs);
    signal?.addEventListener(
      "abort",
      () => {
        clearTimeout(timer);
        reject(signal.reason);
      },
      { once: true },
    );
  });
}

function withTrailingSlash(value: string): string {
  return value.endsWith("/") ? value : `${value}/`;
}

function requiredHttpUrl(value: unknown, label: string): string {
  const raw = requiredString(value, label);
  const url = new URL(raw);
  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error(`${label} must use http or https`);
  }
  url.username = "";
  url.password = "";
  return url.toString();
}

function safeComfyPathPart(
  value: unknown,
  label: string,
  allowEmpty = false,
): string {
  const raw = typeof value === "string" ? value : "";
  if (
    (!allowEmpty && raw.length === 0) ||
    raw.includes("\0") ||
    raw.includes("\\") ||
    raw.startsWith("/") ||
    raw.split("/").some((segment) => segment === "..")
  ) {
    throw new ImageGenerationError(
      "comfyui_invalid_output_descriptor",
      `ComfyUI output ${label} is invalid`,
      false,
    );
  }
  return raw;
}

function stableName(value: unknown, label: string): string {
  const raw = requiredString(value, label);
  if (!/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(raw)) {
    throw new Error(`${label} must be a stable id`);
  }
  return raw;
}

function requiredString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${label} must be a non-empty string`);
  }
  return value.trim();
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim()
    : undefined;
}

function optionalInteger(value: unknown, label: string): number | undefined {
  if (value === undefined || value === null) return undefined;
  if (!Number.isSafeInteger(value))
    throw new Error(`${label} must be an integer`);
  return value as number;
}

function boundedInteger(
  value: unknown,
  fallback: number,
  minimum: number,
  maximum: number,
): number {
  const resolved =
    value === undefined ? fallback : optionalInteger(value, "value");
  if (resolved === undefined || resolved < minimum || resolved > maximum) {
    throw new Error(`integer value must be within ${minimum}..${maximum}`);
  }
  return resolved;
}

function requiredArray(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${label} must be an array`);
  return value;
}

function optionalStringArray(value: unknown): string[] {
  if (value === undefined || value === null) return [];
  return requiredArray(value, "string array").map((item) =>
    stableName(item, "string array item"),
  );
}

function requiredRecord(
  value: unknown,
  label: string,
): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value as Record<string, unknown>;
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function appendSuffix(value: string, suffix: string | undefined): string {
  return [value.trim(), suffix?.trim()].filter(Boolean).join(", ");
}

function assertRange(
  value: number,
  minimum: number,
  maximum: number,
  label: string,
): void {
  if (value < minimum || value > maximum) {
    throw new Error(`${label} must be within ${minimum}..${maximum}`);
  }
}

function safeJson(value: unknown): string {
  try {
    return JSON.stringify(value).slice(0, 2_000);
  } catch {
    return String(value).slice(0, 2_000);
  }
}
