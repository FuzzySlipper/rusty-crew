import { Agent } from "@earendil-works/pi-agent-core";
import type { AgentOptions } from "@earendil-works/pi-agent-core";
import {
  registerBuiltInApiProviders,
  streamSimple,
  type Api,
  type Model,
} from "@earendil-works/pi-ai";
import type { PiAgentFactory } from "./pi-agent-brain.js";

const DEFAULT_DEN_ROUTER_URL = "http://127.0.0.1:18082";
const DEFAULT_MODEL_CANDIDATES = [
  "deepseek-flash",
  "grok",
  "glm",
  "local-coder",
] as const;

interface DenRouterModel {
  id: string;
  context_length?: number;
}

interface DenRouterModelsResponse {
  data?: DenRouterModel[];
}

interface DenRouterRoutesResponse {
  models?: Record<
    string,
    {
      backends?: Array<{ type?: string; healthy?: boolean; drained?: boolean }>;
    }
  >;
}

export interface DenRouterAgentOptions {
  baseUrl?: string;
  modelId?: string;
  api?: string;
  apiKeyEnv?: string;
  maxTokens?: number;
  temperature?: number;
}

export interface DenRouterModelSelection {
  model: Model<Api>;
  baseUrl: string;
}

let builtInsRegistered = false;

export async function createDenRouterPiAgentFactory(
  options: DenRouterAgentOptions = {},
): Promise<PiAgentFactory> {
  const selection = await resolveDenRouterModel(options);
  registerApiProvidersOnce();
  const apiKey =
    options.apiKeyEnv === undefined
      ? "den-router"
      : (process.env[options.apiKeyEnv] ?? "den-router");

  return (agentOptions: AgentOptions) =>
    new Agent({
      ...agentOptions,
      getApiKey: () => apiKey,
      initialState: {
        ...agentOptions.initialState,
        model: selection.model,
      },
      streamFn: (model, context, streamOptions) =>
        streamSimple(model, context, {
          ...streamOptions,
          apiKey,
          temperature: options.temperature ?? streamOptions?.temperature ?? 0,
          maxTokens: options.maxTokens ?? streamOptions?.maxTokens ?? 128,
        }),
    });
}

export async function resolveDenRouterModel(
  options: DenRouterAgentOptions = {},
): Promise<DenRouterModelSelection> {
  const baseUrl = normalizeBaseUrl(
    options.baseUrl ?? process.env.DEN_ROUTER_URL,
  );
  const [models, routes] = await Promise.all([
    fetchDenRouterModels(baseUrl),
    fetchDenRouterRoutes(baseUrl),
  ]);
  const selected = selectModel(
    models,
    options.modelId ?? process.env.RUSTY_CREW_DEN_ROUTER_MODEL,
  );
  const api =
    normalizeApi(options.api) ??
    (isCodexBacked(selected.id, routes)
      ? "openai-responses"
      : "openai-completions");

  return {
    baseUrl,
    model: {
      id: selected.id,
      name: selected.id,
      api,
      provider: "den-router",
      baseUrl: `${baseUrl}/v1`,
      reasoning: api === "openai-responses",
      input: ["text"],
      cost: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0 },
      contextWindow: selected.context_length ?? 128_000,
      maxTokens: options.maxTokens ?? 128,
      compat: {
        supportsDeveloperRole: false,
        supportsReasoningEffort: false,
        supportsUsageInStreaming: false,
      },
    } satisfies Model<Api>,
  };
}

function normalizeApi(raw: string | undefined): Api | undefined {
  if (raw === undefined) return undefined;
  if (raw === "openai-responses" || raw === "openai-completions") {
    return raw;
  }
  throw new Error(`unsupported den-router api ${raw}`);
}

async function fetchDenRouterModels(
  baseUrl: string,
): Promise<DenRouterModel[]> {
  const response = await fetch(`${baseUrl}/v1/models`);
  if (!response.ok) {
    throw new Error(`den-router /v1/models returned HTTP ${response.status}`);
  }

  const payload = (await response.json()) as DenRouterModelsResponse;
  if (!Array.isArray(payload.data) || payload.data.length === 0) {
    throw new Error("den-router returned no models");
  }

  return payload.data;
}

async function fetchDenRouterRoutes(
  baseUrl: string,
): Promise<DenRouterRoutesResponse | undefined> {
  try {
    const response = await fetch(`${baseUrl}/routes`);
    if (!response.ok) {
      return undefined;
    }
    const payload = (await response.json()) as DenRouterRoutesResponse;
    return payload && typeof payload === "object" ? payload : undefined;
  } catch {
    return undefined;
  }
}

function selectModel(
  models: DenRouterModel[],
  requested?: string,
): DenRouterModel {
  if (requested) {
    const selected = models.find((model) => model.id === requested);
    if (!selected) {
      throw new Error(`den-router model ${requested} is not available`);
    }
    return selected;
  }

  for (const candidate of DEFAULT_MODEL_CANDIDATES) {
    const selected = models.find((model) => model.id === candidate);
    if (selected) {
      return selected;
    }
  }

  return models[0]!;
}

function isCodexBacked(
  modelId: string,
  routes: DenRouterRoutesResponse | undefined,
): boolean {
  const backends = routes?.models?.[modelId]?.backends;
  return Array.isArray(backends)
    ? backends.some((backend) => backend.type === "codex-oauth")
    : false;
}

function normalizeBaseUrl(raw: string | undefined): string {
  return (raw ?? DEFAULT_DEN_ROUTER_URL)
    .replace(/\/$/, "")
    .replace(/\/v1$/i, "");
}

function registerApiProvidersOnce(): void {
  if (builtInsRegistered) {
    return;
  }

  registerBuiltInApiProviders();
  builtInsRegistered = true;
}
