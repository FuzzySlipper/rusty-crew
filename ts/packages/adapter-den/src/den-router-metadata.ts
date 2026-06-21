import type {
  AdapterId,
  ChannelProviderRefs,
  ChannelRuntimeIdentity,
  DenRouterMetadataProjection,
  ExternalBindingStatus,
  ProjectId,
  ResultReference,
  WorkReference,
  WorkReferenceKind,
} from "@rusty-crew/contracts";

export type DenProductWorkRefKind =
  | "project"
  | "task"
  | "assignment"
  | "run"
  | "channel_binding"
  | "channel_message";

export interface DenProductWorkRefInput {
  refKind: DenProductWorkRefKind | WorkReferenceKind | string;
  id: string;
  projectId?: ProjectId | string;
  label?: string;
  externalUrl?: string;
}

export interface DenRouterMetadataProjectionInput {
  adapterId: AdapterId;
  bindingId: string;
  runtime: ChannelRuntimeIdentity;
  providerRefs?: Partial<ChannelProviderRefs>;
  denWorkRefs: readonly DenProductWorkRefInput[];
  resultRefs?: readonly ResultReference[];
  toolProfileKey?: string;
  mcpSurfaceRefs?: readonly string[];
  status: ExternalBindingStatus;
  degradedReason?: string;
  observedAt?: string;
  provenance?: Record<string, unknown>;
}

const SENSITIVE_PROVENANCE_KEY =
  /(token|secret|password|credential|prompt|raw.?output|tool.?output)/i;

export function denProductWorkRef(
  input: DenProductWorkRefInput,
): WorkReference {
  return {
    kind: "work_ref.v1",
    sourceDomain: "den",
    refKind: input.refKind,
    id: input.id,
    projectId: input.projectId,
    label: input.label,
    externalUrl: input.externalUrl,
  };
}

export function createDenRouterMetadataProjection(
  input: DenRouterMetadataProjectionInput,
): DenRouterMetadataProjection {
  return {
    kind: "den_router_metadata_projection.v1",
    adapterId: input.adapterId,
    bindingId: input.bindingId,
    runtime: input.runtime,
    providerRefs: input.providerRefs,
    workRefs: input.denWorkRefs.map(denProductWorkRef),
    resultRefs: input.resultRefs ? [...input.resultRefs] : undefined,
    toolProfileKey: input.toolProfileKey,
    mcpSurfaceRefs: input.mcpSurfaceRefs
      ? [...input.mcpSurfaceRefs]
      : undefined,
    status: input.status,
    degradedReason: input.degradedReason,
    observedAt: input.observedAt ?? new Date().toISOString(),
    provenance: sanitizeRouterMetadataProvenance(input.provenance ?? {}),
  };
}

export function sanitizeRouterMetadataProvenance(
  provenance: Record<string, unknown>,
): Record<string, unknown> {
  return Object.fromEntries(
    Object.entries(provenance).map(([key, value]) => [
      key,
      SENSITIVE_PROVENANCE_KEY.test(key) ? "[redacted]" : value,
    ]),
  );
}
