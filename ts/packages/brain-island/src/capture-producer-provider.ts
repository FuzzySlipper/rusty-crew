import type {
  MemorySpaceId,
  ProfileId,
  SessionActivityDigest,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type {
  CaptureTargetSpaceId,
  TypedCaptureMemoryProposal,
} from "./capture-memory-proposals.js";
import type { BackgroundReviewDenseMemoryRecord } from "./background-memory-skill-review.js";
import type { LoadedSkill } from "./profile-loading.js";
import { resolveModelConfigurationForBrain } from "./model-runtime-resolution.js";

export interface CaptureProducerProviderInput {
  runId: string;
  profileId: ProfileId | string;
  modelConfigId: string;
  bridge: Pick<
    NativeBridgeModule,
    | "getModelConfiguration"
    | "getModelEndpoint"
    | "getServiceCredential"
    | "getServiceCredentialSecret"
  >;
  sessionActivityDigests: readonly SessionActivityDigest[];
  denseProfileMemory?: readonly BackgroundReviewDenseMemoryRecord[];
  skills?: readonly LoadedSkill[];
  maxProposals?: number;
  allowedSpaces?: readonly CaptureTargetSpaceId[];
  timeoutMs?: number;
  transport?: CaptureProviderJsonTransport;
}

export interface CaptureProducerProviderResult {
  proposals: TypedCaptureMemoryProposal[];
  skippedReasons: string[];
}

export type CaptureProviderJsonTransport = (
  url: string,
  init: {
    method: "POST";
    headers: Record<string, string>;
    body: string;
    signal?: AbortSignal;
  },
) => Promise<unknown>;

interface ProviderProposalOutput {
  proposals?: unknown;
  skippedReasons?: unknown;
}

const DEFAULT_MAX_CAPTURE_PROPOSALS = 8;
const DEFAULT_TIMEOUT_MS = 30_000;

export async function runStructuredCaptureProvider(
  input: CaptureProducerProviderInput,
): Promise<CaptureProducerProviderResult> {
  const modelConfigId = input.modelConfigId.trim();
  if (!modelConfigId) {
    return {
      proposals: [],
      skippedReasons: ["capture_model_config_id_missing"],
    };
  }
  if (input.sessionActivityDigests.length === 0) {
    return {
      proposals: [],
      skippedReasons: ["capture_no_session_activity_digests"],
    };
  }
  let modelConfig;
  try {
    modelConfig = await resolveModelConfigurationForBrain(
      input.bridge,
      modelConfigId,
    );
  } catch {
    return {
      proposals: [],
      skippedReasons: ["capture_model_configuration_unavailable"],
    };
  }
  if (modelConfig.api !== "openai-completions") {
    return {
      proposals: [],
      skippedReasons: ["capture_model_protocol_unsupported"],
    };
  }
  const secret =
    modelConfig.apiKeyEnv === undefined
      ? undefined
      : process.env[modelConfig.apiKeyEnv];
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(),
    input.timeoutMs ?? DEFAULT_TIMEOUT_MS,
  );
  try {
    const output = (await (input.transport ?? fetchJson)(
      `${(modelConfig.baseUrl ?? "https://api.openai.com/v1").replace(/\/+$/, "")}/chat/completions`,
      {
        method: "POST",
        headers: {
          "content-type": "application/json",
          ...(secret ? { authorization: `Bearer ${secret}` } : {}),
        },
        body: JSON.stringify({
          model: modelConfig.modelName,
          temperature:
            modelConfig.temperatureMilli === undefined
              ? undefined
              : modelConfig.temperatureMilli / 1_000,
          max_tokens: modelConfig.maxOutputTokens,
          response_format: { type: "json_object" },
          messages: [
            {
              role: "system",
              content: captureProducerSystemPrompt(
                input.allowedSpaces ?? ["profile_dense"],
              ),
            },
            {
              role: "user",
              content: captureProducerUserPrompt(input),
            },
          ],
        }),
        signal: controller.signal,
      },
    )) as ProviderProposalOutput;
    return normalizeCaptureProviderOutput({
      output,
      runId: input.runId,
      profileId: input.profileId,
      allowedSpaces: input.allowedSpaces ?? ["profile_dense"],
      maxProposals: input.maxProposals ?? DEFAULT_MAX_CAPTURE_PROPOSALS,
    });
  } catch (error) {
    return {
      proposals: [],
      skippedReasons: [
        error instanceof DOMException && error.name === "AbortError"
          ? "capture_provider_timeout"
          : "capture_provider_unavailable",
      ],
    };
  } finally {
    clearTimeout(timeout);
  }
}

export function normalizeCaptureProviderOutput(input: {
  output: ProviderProposalOutput;
  runId: string;
  profileId: ProfileId | string;
  allowedSpaces?: readonly CaptureTargetSpaceId[];
  maxProposals?: number;
}): CaptureProducerProviderResult {
  const skippedReasons = stringArray(input.output.skippedReasons);
  const rawProposals = Array.isArray(input.output.proposals)
    ? input.output.proposals
    : undefined;
  if (rawProposals === undefined) {
    return {
      proposals: [],
      skippedReasons: [...skippedReasons, "capture_provider_invalid_json"],
    };
  }
  const allowed = new Set(input.allowedSpaces ?? ["profile_dense"]);
  const proposals: TypedCaptureMemoryProposal[] = [];
  const maxProposals = input.maxProposals ?? DEFAULT_MAX_CAPTURE_PROPOSALS;
  for (const raw of rawProposals) {
    if (proposals.length >= maxProposals) break;
    const proposal = typedProposal(raw);
    if (!proposal || !allowed.has(proposal.space_id)) continue;
    proposals.push(proposal);
  }
  return {
    proposals,
    skippedReasons:
      proposals.length === 0
        ? [...skippedReasons, "capture_no_supported_proposals"]
        : skippedReasons,
  };
}

function captureProducerSystemPrompt(
  allowedSpaces: readonly CaptureTargetSpaceId[],
): string {
  return [
    "Analyze recent session activity and propose durable memory candidates.",
    "Return only JSON with a proposals array and optional skippedReasons array.",
    `Allowed memory spaces: ${allowedSpaces.join(", ")}.`,
    "Do not capture one-time fixes, current service status, secrets, or transient logs.",
    "Every proposal needs evidence_refs, confidence between 0 and 1, and durability_rationale.",
  ].join("\n");
}

function captureProducerUserPrompt(
  input: CaptureProducerProviderInput,
): string {
  return JSON.stringify({
    profileId: input.profileId,
    currentDenseProfileMemory: input.denseProfileMemory ?? [],
    skills: (input.skills ?? []).map((skill) => ({
      slug: skill.slug,
      title: skill.title,
      summary: skill.summary,
    })),
    sessionActivityDigests: input.sessionActivityDigests.map((digest) => ({
      digest_id: digest.digest_id,
      wake_id: digest.wake_id,
      source: digest.source,
      summary_text: digest.summary_text,
      signals_json: digest.signals_json,
      tool_calls_json: digest.tool_calls_json,
      allowed_capture_spaces: digest.allowed_capture_spaces,
    })),
  });
}

async function fetchJson(
  url: string,
  init: Parameters<CaptureProviderJsonTransport>[1],
): Promise<unknown> {
  const response = await fetch(url, init);
  if (!response.ok) {
    throw new Error(`capture provider returned HTTP ${response.status}`);
  }
  const body = (await response.json()) as unknown;
  return openAiChatJsonOutput(body);
}

function openAiChatJsonOutput(body: unknown): unknown {
  const content = firstChoiceContent(body);
  if (content === undefined) return body;
  try {
    return JSON.parse(content) as unknown;
  } catch {
    throw new Error("capture provider returned invalid JSON content");
  }
}

function firstChoiceContent(body: unknown): string | undefined {
  if (!isRecord(body) || !Array.isArray(body.choices)) return undefined;
  const first = body.choices[0];
  if (!isRecord(first) || !isRecord(first.message)) return undefined;
  return typeof first.message.content === "string"
    ? first.message.content
    : undefined;
}

function typedProposal(input: unknown): TypedCaptureMemoryProposal | undefined {
  if (!isRecord(input)) return undefined;
  const spaceId = stringValue(input.space_id) as
    | CaptureTargetSpaceId
    | undefined;
  const operation = stringValue(input.operation);
  const scope = isRecord(input.scope) ? input.scope : undefined;
  const shape = isRecord(input.shape) ? input.shape : undefined;
  const confidence = numberValue(input.confidence);
  if (
    !spaceId ||
    !operation ||
    !scope ||
    !shape ||
    confidence === undefined ||
    !Array.isArray(input.evidence_refs)
  ) {
    return undefined;
  }
  return {
    id: stringValue(input.id),
    summary: stringValue(input.summary) ?? "capture proposal",
    space_id: spaceId,
    operation: operation as TypedCaptureMemoryProposal["operation"],
    scope: {
      scope_type: stringValue(scope.scope_type) as never,
      scope_id: stringValue(scope.scope_id) ?? "",
    },
    shape: {
      shape_id: stringValue(shape.shape_id) as never,
      version: positiveIntegerValue(shape.version) ?? 1,
    },
    content: input.content,
    evidence_refs:
      input.evidence_refs as TypedCaptureMemoryProposal["evidence_refs"],
    confidence,
    durability_rationale:
      stringValue(input.durability_rationale) ??
      stringValue(input.durabilityRationale) ??
      "",
    governance_policy: stringValue(
      input.governance_policy,
    ) as TypedCaptureMemoryProposal["governance_policy"],
    dedupe_key: stringValue(input.dedupe_key),
  };
}

function stringArray(input: unknown): string[] {
  return Array.isArray(input)
    ? input.flatMap((value) => (typeof value === "string" ? [value] : []))
    : [];
}

function stringValue(input: unknown): string | undefined {
  return typeof input === "string" && input.trim() ? input.trim() : undefined;
}

function numberValue(input: unknown): number | undefined {
  return typeof input === "number" && Number.isFinite(input)
    ? Math.max(0, Math.min(1, input))
    : undefined;
}

function positiveIntegerValue(input: unknown): number | undefined {
  return typeof input === "number" && Number.isInteger(input) && input > 0
    ? input
    : undefined;
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return input !== null && typeof input === "object" && !Array.isArray(input);
}
