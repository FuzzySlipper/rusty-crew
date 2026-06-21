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

export interface DenRouterMetadataQuery {
  bindingId?: string;
  adapterId?: AdapterId | string;
  agentId?: string;
  sessionId?: string;
  profileId?: string;
  provider?: string;
  externalChannelId?: string;
  status?: ExternalBindingStatus;
  limit?: number;
}

export interface DenRouterMetadataQueryResult {
  generatedAt: string;
  total: number;
  items: DenRouterMetadataProjection[];
}

export interface DenRouterMetadataReader {
  queryRouterMetadata(
    query?: DenRouterMetadataQuery,
  ): DenRouterMetadataQueryResult;
}

export interface DenRouterMetadataStore extends DenRouterMetadataReader {
  upsertRouterMetadata(projection: DenRouterMetadataProjection): void;
}

export interface MemoryDenRouterMetadataStoreOptions {
  now?: () => string;
  maxRecords?: number;
  defaultLimit?: number;
  maxLimit?: number;
}

const SENSITIVE_PROVENANCE_KEY =
  /(token|secret|password|credential|prompt|raw.?output|tool.?output)/i;

export function createMemoryDenRouterMetadataStore(
  options: MemoryDenRouterMetadataStoreOptions = {},
): DenRouterMetadataStore {
  const records = new Map<string, DenRouterMetadataProjection>();
  const now = options.now ?? (() => new Date().toISOString());
  const maxRecords = options.maxRecords ?? 1_000;
  const defaultLimit = options.defaultLimit ?? 50;
  const maxLimit = options.maxLimit ?? 200;

  return {
    upsertRouterMetadata(projection): void {
      records.set(metadataKey(projection), sanitizeProjection(projection));
      if (records.size > maxRecords) {
        const overflow = records.size - maxRecords;
        for (const key of [...records.keys()].slice(0, overflow)) {
          records.delete(key);
        }
      }
    },

    queryRouterMetadata(query = {}): DenRouterMetadataQueryResult {
      const limit = clamp(query.limit ?? defaultLimit, 1, maxLimit);
      const items = [...records.values()]
        .filter((projection) => matchesRouterMetadataQuery(projection, query))
        .sort((left, right) => right.observedAt.localeCompare(left.observedAt));
      return {
        generatedAt: now(),
        total: items.length,
        items: items.slice(0, limit).map(copyProjection),
      };
    },
  };
}

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

function matchesRouterMetadataQuery(
  projection: DenRouterMetadataProjection,
  query: DenRouterMetadataQuery,
): boolean {
  if (query.bindingId && projection.bindingId !== query.bindingId) {
    return false;
  }
  if (query.adapterId && projection.adapterId !== query.adapterId) {
    return false;
  }
  if (query.agentId && projection.runtime.agentId !== query.agentId) {
    return false;
  }
  if (query.sessionId && projection.runtime.sessionId !== query.sessionId) {
    return false;
  }
  if (query.profileId && projection.runtime.profileId !== query.profileId) {
    return false;
  }
  if (query.status && projection.status !== query.status) {
    return false;
  }
  if (query.provider && projection.providerRefs?.provider !== query.provider) {
    return false;
  }
  if (
    query.externalChannelId &&
    projection.providerRefs?.externalChannelId !== query.externalChannelId
  ) {
    return false;
  }
  return true;
}

function sanitizeProjection(
  projection: DenRouterMetadataProjection,
): DenRouterMetadataProjection {
  return {
    ...projection,
    workRefs: projection.workRefs.map((ref) => ({ ...ref })),
    resultRefs: projection.resultRefs?.map((ref) => ({ ...ref })),
    mcpSurfaceRefs: projection.mcpSurfaceRefs
      ? [...projection.mcpSurfaceRefs]
      : undefined,
    providerRefs: projection.providerRefs
      ? { ...projection.providerRefs }
      : undefined,
    runtime: { ...projection.runtime },
    provenance: sanitizeRouterMetadataProvenance(projection.provenance),
  };
}

function copyProjection(
  projection: DenRouterMetadataProjection,
): DenRouterMetadataProjection {
  return sanitizeProjection(projection);
}

function metadataKey(projection: DenRouterMetadataProjection): string {
  return `${projection.adapterId}:${projection.bindingId}`;
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, Math.trunc(value)));
}
