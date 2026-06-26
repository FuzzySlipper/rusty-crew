import type {
  MemoryGovernanceDecisionInput,
  MemoryProposalEnvelope,
  MemoryProposalQuery,
  MemorySpaceDescriptor,
  MemorySpaceId,
  SessionState,
} from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeProfileMemoryRecord,
  NativeSessionMemoryPromptContext,
  NativeSessionMemoryRecord,
} from "@rusty-crew/native-bridge";
import { Type, type Static } from "typebox";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

const MAX_LIMIT = 100;
const DEFAULT_LIMIT = 25;
const PROFILE_DENSE_SPACE_ID = "profile_dense";
const SESSION_MEMORY_SPACE_ID = "session_memory";

export interface MemorySpaceReadContext {
  bridge: Pick<
    NativeBridgeModule,
    | "getProfileMemory"
    | "buildSessionMemoryPromptContext"
    | "listMemoryProposals"
    | "listMemorySpaceDescriptors"
    | "listProfileMemory"
    | "querySessionMemoryRecords"
    | "recordMemoryGovernanceDecision"
    | "saveMemoryProposal"
  >;
}

export interface MemorySpaceAdminRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId?: string;
}

export interface MemorySpaceRecordQuery {
  profileId?: string;
  targetType?: "profile" | "user";
  targetId?: string;
  sessionId?: string;
  branchId?: string;
  activeBranchId?: string;
  includeAncestors?: boolean;
  includeSiblings?: boolean;
  shapeId?: string;
  promptContextOnly?: boolean;
  limit: number;
  offset: number;
}

export interface MemorySpaceRecordListResult {
  space_id: MemorySpaceId | string;
  read_only: true;
  source: "rust_bridge_memory_space";
  items: Array<NativeProfileMemoryRecord | NativeSessionMemoryRecord>;
  diagnostics?: NativeSessionMemoryPromptContext["diagnostics"];
  total: number;
  limit: number;
  offset: number;
  nextOffset?: number;
}

export interface MemorySpaceRecordReadResult {
  space_id: MemorySpaceId | string;
  read_only: true;
  source: "rust_bridge_memory_space";
  item?: NativeProfileMemoryRecord | NativeSessionMemoryRecord;
}

export interface MemorySpaceCatalogResult {
  schema_version: 1;
  source: "rust_bridge_memory_space";
  items: MemorySpaceDescriptor[];
  total: number;
}

export type MemorySpaceToolDetails =
  | MemorySpaceCatalogResult
  | MemorySpaceRecordListResult
  | MemorySpaceRecordReadResult
  | {
      ok: false;
      reason_code:
        | "invalid_memory_space_input"
        | "invalid_memory_space_key"
        | "missing_required_parameter"
        | "memory_space_not_found"
        | "memory_space_records_unsupported"
        | "target_id_required";
      message: string;
    };

export async function handleMemorySpaceAdminRequest(
  request: MemorySpaceAdminRequest,
  context: MemorySpaceReadContext,
): Promise<AdminRouteResult> {
  const requestId = request.requestId ?? "memory-space";
  const method = request.method.toUpperCase();

  const url = new URL(request.url, "http://rusty-crew.local");
  try {
    if (url.pathname.startsWith("/v1/admin/memory/proposals")) {
      return await handleMemoryProposalAdminRequest(
        { method, url, body: request.body, requestId },
        context,
      );
    }
    if (method !== "GET") {
      return routeFailure(405, requestId, {
        reason_code: "memory_space_read_only",
        message: "memory-space routes are read-only and only support GET",
      });
    }
    const descriptors = await context.bridge.listMemorySpaceDescriptors();
    const match = matchMemorySpaceRoute(url.pathname);
    if (match.kind === "catalog") {
      return routeSuccess(requestId, catalog(descriptors));
    }
    if (match.kind === "unknown") {
      return routeFailure(404, requestId, {
        reason_code: "unknown_memory_space_route",
        message: `unknown memory-space route ${url.pathname}`,
      });
    }

    const descriptor = findDescriptor(descriptors, match.spaceId);
    if (!descriptor) {
      return routeFailure(404, requestId, {
        reason_code: "memory_space_not_found",
        message: `memory space ${match.spaceId} is not registered`,
      });
    }
    if (match.kind === "descriptor") {
      return routeSuccess(requestId, descriptor);
    }
    if (match.kind === "records") {
      const query = parseMemorySpaceRecordQuery(descriptor, url.searchParams);
      return routeSuccess(
        requestId,
        await listMemorySpaceRecords(descriptor, query, context),
      );
    }
    const query = parseMemorySpaceRecordQuery(descriptor, url.searchParams);
    return routeSuccess(
      requestId,
      await readMemorySpaceRecord(descriptor, query, match.key, context),
    );
  } catch (error) {
    return routeFailure(400, requestId, {
      reason_code:
        error instanceof MemorySpaceInputError
          ? error.reasonCode
          : "invalid_memory_space_input",
      message:
        error instanceof Error ? error.message : "invalid memory-space input",
    });
  }
}

async function handleMemoryProposalAdminRequest(
  request: {
    method: string;
    url: URL;
    body?: unknown;
    requestId: string;
  },
  context: MemorySpaceReadContext,
): Promise<AdminRouteResult> {
  const match = matchMemoryProposalRoute(request.url.pathname);
  if (match.kind === "unknown") {
    return routeFailure(404, request.requestId, {
      reason_code: "unknown_memory_proposal_route",
      message: `unknown memory proposal route ${request.url.pathname}`,
    });
  }
  if (match.kind === "catalog") {
    if (request.method === "GET") {
      return routeSuccess(
        request.requestId,
        await context.bridge.listMemoryProposals(
          parseMemoryProposalQuery(request.url.searchParams),
        ),
      );
    }
    if (request.method === "POST") {
      return routeSuccess(
        request.requestId,
        await context.bridge.saveMemoryProposal(
          memoryProposalBody(request.body),
        ),
      );
    }
    return routeFailure(405, request.requestId, {
      reason_code: "memory_proposal_method_not_allowed",
      message: "memory proposal catalog supports GET and POST",
    });
  }
  if (request.method !== "POST") {
    return routeFailure(405, request.requestId, {
      reason_code: "memory_proposal_decision_method_not_allowed",
      message: "memory proposal decisions only support POST",
    });
  }
  const decision = memoryGovernanceDecisionBody(request.body);
  if (decision.proposal_id !== match.proposalId) {
    return routeFailure(400, request.requestId, {
      reason_code: "memory_proposal_id_mismatch",
      message: "memory governance decision proposal_id must match the route",
    });
  }
  return routeSuccess(
    request.requestId,
    await context.bridge.recordMemoryGovernanceDecision(decision),
  );
}

export async function listMemorySpaceRecords(
  descriptor: MemorySpaceDescriptor,
  query: MemorySpaceRecordQuery,
  context: MemorySpaceReadContext,
): Promise<MemorySpaceRecordListResult> {
  const { items, diagnostics } = await listMemorySpaceRecordItems(
    descriptor,
    query,
    context,
  );
  return {
    space_id: descriptor.space_id,
    read_only: true,
    source: "rust_bridge_memory_space",
    items,
    ...(diagnostics ? { diagnostics } : {}),
    total: items.length,
    limit: query.limit,
    offset: query.offset,
    ...(items.length === query.limit
      ? { nextOffset: query.offset + items.length }
      : {}),
  };
}

export async function readMemorySpaceRecord(
  descriptor: MemorySpaceDescriptor,
  query: Omit<MemorySpaceRecordQuery, "limit" | "offset">,
  key: string,
  context: MemorySpaceReadContext,
): Promise<MemorySpaceRecordReadResult> {
  if (key.trim().length === 0) {
    throw new MemorySpaceInputError(
      "invalid_memory_space_key",
      "memory-space record key must not be empty",
    );
  }
  const item =
    descriptor.space_id === PROFILE_DENSE_SPACE_ID
      ? await context.bridge.getProfileMemory({
          profileId:
            query.profileId ??
            missing("profileId is required for profile_dense record reads"),
          targetType: query.targetType ?? "profile",
          targetId: query.targetId,
          key,
        })
      : descriptor.space_id === SESSION_MEMORY_SPACE_ID
        ? (
            await context.bridge.querySessionMemoryRecords({
              session_id:
                query.sessionId ??
                missing("sessionId is required for session_memory record reads"),
              include_archived: true,
              include_superseded: true,
              page: { limit: MAX_LIMIT, offset: 0 },
            })
          ).find((record) => record.record_id === key)
        : unsupportedMemorySpaceRecords(descriptor);
  return {
    space_id: descriptor.space_id,
    read_only: true,
    source: "rust_bridge_memory_space",
    item,
  };
}

const catalogToolParameters = Type.Object({});
type CatalogToolParams = Static<typeof catalogToolParameters>;

export function memorySpaceCatalogTool(
  context: MemorySpaceReadContext,
): BrainTool<typeof catalogToolParameters, MemorySpaceToolDetails> {
  return {
    name: "memory_space_catalog",
    label: "Memory space catalog",
    description:
      "List Rusty Crew runtime-owned memory spaces. This does not read or proxy Den memory.",
    parameters: catalogToolParameters,
    execute: async (_callId, _params: CatalogToolParams) =>
      toolResult(catalog(await context.bridge.listMemorySpaceDescriptors())),
  };
}

const readToolParameters = Type.Object({
  spaceId: Type.String({ minLength: 1 }),
  profileId: Type.Optional(Type.String({ minLength: 1 })),
  targetType: Type.Optional(
    Type.Union([Type.Literal("profile"), Type.Literal("user")]),
  ),
  targetId: Type.Optional(Type.String({ minLength: 1 })),
  sessionId: Type.Optional(Type.String({ minLength: 1 })),
  activeBranchId: Type.Optional(Type.String({ minLength: 1 })),
  includeAncestors: Type.Optional(Type.Boolean()),
  includeSiblings: Type.Optional(Type.Boolean()),
  shapeId: Type.Optional(Type.String({ minLength: 1 })),
  promptContextOnly: Type.Optional(Type.Boolean()),
  key: Type.Optional(Type.String({ minLength: 1 })),
  limit: Type.Optional(Type.Number({ minimum: 1 })),
  offset: Type.Optional(Type.Number({ minimum: 0 })),
});
type ReadToolParams = Static<typeof readToolParameters>;

export function memorySpaceReadTool(input: {
  context: MemorySpaceReadContext;
  session?: Pick<SessionState, "profileId">;
}): BrainTool<typeof readToolParameters, MemorySpaceToolDetails> {
  return {
    name: "memory_space_read",
    label: "Read memory space",
    description:
      "Read supported Rusty Crew memory-space records through read-only runtime APIs.",
    parameters: readToolParameters,
    execute: async (_callId, params: ReadToolParams) => {
      try {
        const descriptors =
          await input.context.bridge.listMemorySpaceDescriptors();
        const descriptor = findDescriptor(descriptors, params.spaceId);
        if (!descriptor) {
          return toolResult({
            ok: false,
            reason_code: "memory_space_not_found",
            message: `memory space ${params.spaceId} is not registered`,
          });
        }
        const query = {
          profileId:
            params.profileId ??
            input.session?.profileId ??
            (descriptor.space_id === PROFILE_DENSE_SPACE_ID
              ? missing(
                  "profileId is required when no session profile is available",
                )
              : undefined),
          targetType: params.targetType,
          targetId: params.targetId,
          sessionId: params.sessionId,
          activeBranchId: params.activeBranchId,
          includeAncestors: params.includeAncestors,
          includeSiblings: params.includeSiblings,
          shapeId: params.shapeId,
          promptContextOnly: params.promptContextOnly,
          limit: boundedInteger(params.limit, DEFAULT_LIMIT, 1, MAX_LIMIT),
          offset: boundedInteger(params.offset, 0, 0, Number.MAX_SAFE_INTEGER),
        };
        if (params.key) {
          return toolResult(
            await readMemorySpaceRecord(
              descriptor,
              query,
              params.key,
              input.context,
            ),
          );
        }
        return toolResult(
          await listMemorySpaceRecords(descriptor, query, input.context),
        );
      } catch (error) {
        return toolResult({
          ok: false,
          reason_code: toolReasonCode(error),
          message:
            error instanceof Error ? error.message : "memory-space read failed",
        });
      }
    },
  };
}

export function createMemorySpaceToolResolver(
  context: MemorySpaceReadContext,
): BrainToolResolver {
  return ({ wake }) => [
    memorySpaceCatalogTool(context),
    memorySpaceReadTool({ context, session: wake.state.session }),
  ];
}

function catalog(
  descriptors: MemorySpaceDescriptor[],
): MemorySpaceCatalogResult {
  return {
    schema_version: 1,
    source: "rust_bridge_memory_space",
    items: descriptors,
    total: descriptors.length,
  };
}

function matchMemorySpaceRoute(
  pathname: string,
):
  | { kind: "catalog" }
  | { kind: "descriptor"; spaceId: string }
  | { kind: "records"; spaceId: string }
  | { kind: "record"; spaceId: string; key: string }
  | { kind: "unknown" } {
  if (pathname === "/v1/admin/memory/spaces") return { kind: "catalog" };
  const prefix = "/v1/admin/memory/spaces/";
  if (!pathname.startsWith(prefix)) return { kind: "unknown" };
  const parts = pathname
    .slice(prefix.length)
    .split("/")
    .map((part) => decodeURIComponent(part))
    .filter((part) => part.length > 0);
  if (parts.length === 1) return { kind: "descriptor", spaceId: parts[0]! };
  if (parts.length === 2 && parts[1] === "records") {
    return { kind: "records", spaceId: parts[0]! };
  }
  if (parts.length === 3 && parts[1] === "records") {
    return { kind: "record", spaceId: parts[0]!, key: parts[2]! };
  }
  return { kind: "unknown" };
}

function matchMemoryProposalRoute(
  pathname: string,
):
  | { kind: "catalog" }
  | { kind: "decision"; proposalId: string }
  | { kind: "unknown" } {
  if (pathname === "/v1/admin/memory/proposals") return { kind: "catalog" };
  const prefix = "/v1/admin/memory/proposals/";
  if (!pathname.startsWith(prefix)) return { kind: "unknown" };
  const parts = pathname
    .slice(prefix.length)
    .split("/")
    .map((part) => decodeURIComponent(part))
    .filter((part) => part.length > 0);
  if (parts.length === 2 && parts[1] === "decisions") {
    return { kind: "decision", proposalId: parts[0]! };
  }
  return { kind: "unknown" };
}

function parseMemoryProposalQuery(
  params: URLSearchParams,
): MemoryProposalQuery {
  return {
    space_id: optionalQueryString(params, "spaceId") as
      | MemorySpaceId
      | undefined,
    status: optionalQueryString(
      params,
      "status",
    ) as MemoryProposalQuery["status"],
    dedupe_key: optionalQueryString(params, "dedupeKey"),
    limit: boundedInteger(
      Number(params.get("limit") ?? DEFAULT_LIMIT),
      DEFAULT_LIMIT,
      1,
      MAX_LIMIT,
    ),
    offset: boundedInteger(
      Number(params.get("offset") ?? 0),
      0,
      0,
      Number.MAX_SAFE_INTEGER,
    ),
  };
}

function memoryProposalBody(body: unknown): MemoryProposalEnvelope {
  if (!isRecord(body)) {
    throw new MemorySpaceInputError(
      "invalid_memory_proposal_input",
      "memory proposal request body must be an object",
    );
  }
  return body as unknown as MemoryProposalEnvelope;
}

function memoryGovernanceDecisionBody(
  body: unknown,
): MemoryGovernanceDecisionInput {
  if (!isRecord(body)) {
    throw new MemorySpaceInputError(
      "invalid_memory_governance_decision_input",
      "memory governance decision request body must be an object",
    );
  }
  return body as unknown as MemoryGovernanceDecisionInput;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function findDescriptor(
  descriptors: readonly MemorySpaceDescriptor[],
  spaceId: string,
): MemorySpaceDescriptor | undefined {
  return descriptors.find((descriptor) => descriptor.space_id === spaceId);
}

async function listMemorySpaceRecordItems(
  descriptor: MemorySpaceDescriptor,
  query: MemorySpaceRecordQuery,
  context: MemorySpaceReadContext,
): Promise<{
  items: Array<NativeProfileMemoryRecord | NativeSessionMemoryRecord>;
  diagnostics?: NativeSessionMemoryPromptContext["diagnostics"];
}> {
  if (descriptor.space_id === PROFILE_DENSE_SPACE_ID) {
    return {
      items: await context.bridge.listProfileMemory({
        profileId:
          query.profileId ??
          missing("profileId is required for profile_dense record reads"),
        targetType: query.targetType,
        targetId: query.targetId,
        limit: query.limit,
        offset: query.offset,
      }),
    };
  }
  if (descriptor.space_id === SESSION_MEMORY_SPACE_ID) {
    const sessionId =
      query.sessionId ??
      missing("sessionId is required for session_memory record reads");
    if (query.promptContextOnly === true || query.activeBranchId !== undefined) {
      const contextResult = await context.bridge.buildSessionMemoryPromptContext(
        {
          session_id: sessionId,
          active_branch_id: query.activeBranchId,
          include_ancestors: query.includeAncestors ?? true,
          include_siblings: query.includeSiblings ?? false,
          shape_id: query.shapeId,
          prompt_context_only: query.promptContextOnly ?? true,
          page: { limit: query.limit, offset: query.offset },
        },
      );
      return {
        items: contextResult.records,
        diagnostics: contextResult.diagnostics,
      };
    }
    return {
      items: await context.bridge.querySessionMemoryRecords({
        session_id: sessionId,
        branch_id: query.branchId,
        shape_id: query.shapeId,
        include_archived: query.promptContextOnly === false,
        include_superseded: query.promptContextOnly === false,
        page: { limit: query.limit, offset: query.offset },
      }),
    };
  }
  return unsupportedMemorySpaceRecords(descriptor);
}

function unsupportedMemorySpaceRecords(
  descriptor: MemorySpaceDescriptor,
): never {
  throw new MemorySpaceInputError(
    "memory_space_records_unsupported",
    `memory space ${descriptor.space_id} does not expose record reads yet`,
  );
}

function parseMemorySpaceRecordQuery(
  descriptor: MemorySpaceDescriptor,
  params: URLSearchParams,
): MemorySpaceRecordQuery {
  if (descriptor.space_id === PROFILE_DENSE_SPACE_ID) {
    return parseProfileDenseQuery(params);
  }
  if (descriptor.space_id === SESSION_MEMORY_SPACE_ID) {
    return parseSessionMemoryQuery(params);
  }
  return unsupportedMemorySpaceRecords(descriptor);
}

function parseProfileDenseQuery(
  params: URLSearchParams,
): MemorySpaceRecordQuery {
  const profileId = requiredQueryString(params, "profileId");
  const targetType = optionalTargetType(params.get("targetType"));
  const targetId = optionalQueryString(params, "targetId");
  if (targetType === "user" && targetId === undefined) {
    throw new MemorySpaceInputError(
      "target_id_required",
      "targetId is required when targetType is user",
    );
  }
  return {
    profileId,
    targetType,
    targetId,
    limit: boundedInteger(
      Number(params.get("limit") ?? DEFAULT_LIMIT),
      DEFAULT_LIMIT,
      1,
      MAX_LIMIT,
    ),
    offset: boundedInteger(
      Number(params.get("offset") ?? 0),
      0,
      0,
      Number.MAX_SAFE_INTEGER,
    ),
  };
}

function parseSessionMemoryQuery(params: URLSearchParams): MemorySpaceRecordQuery {
  return {
    sessionId: requiredQueryString(params, "sessionId"),
    branchId: optionalQueryString(params, "branchId"),
    activeBranchId: optionalQueryString(params, "activeBranchId"),
    includeAncestors: optionalBoolean(params, "includeAncestors"),
    includeSiblings: optionalBoolean(params, "includeSiblings"),
    shapeId: optionalQueryString(params, "shapeId"),
    promptContextOnly: optionalBoolean(params, "promptContextOnly"),
    limit: boundedInteger(
      Number(params.get("limit") ?? DEFAULT_LIMIT),
      DEFAULT_LIMIT,
      1,
      MAX_LIMIT,
    ),
    offset: boundedInteger(
      Number(params.get("offset") ?? 0),
      0,
      0,
      Number.MAX_SAFE_INTEGER,
    ),
  };
}

function optionalTargetType(
  value: string | null,
): "profile" | "user" | undefined {
  if (value === null || value === "") return undefined;
  if (value === "profile" || value === "user") return value;
  throw new MemorySpaceInputError(
    "invalid_target_type",
    "targetType must be profile or user",
  );
}

function requiredQueryString(params: URLSearchParams, name: string): string {
  const value = optionalQueryString(params, name);
  if (value === undefined) {
    throw new MemorySpaceInputError(
      "missing_required_parameter",
      `${name} is required`,
    );
  }
  return value;
}

function optionalQueryString(
  params: URLSearchParams,
  name: string,
): string | undefined {
  const value = params.get(name);
  if (value === null || value.trim().length === 0) return undefined;
  return value;
}

function optionalBoolean(
  params: URLSearchParams,
  name: string,
): boolean | undefined {
  const value = params.get(name);
  if (value === null || value.trim().length === 0) return undefined;
  if (value === "true" || value === "1") return true;
  if (value === "false" || value === "0") return false;
  throw new MemorySpaceInputError(
    "invalid_memory_space_input",
    `${name} must be true or false`,
  );
}

function boundedInteger(
  value: number | undefined,
  fallback: number,
  min: number,
  max: number,
): number {
  if (value === undefined || !Number.isFinite(value)) return fallback;
  return Math.max(min, Math.min(Math.trunc(value), max));
}

function missing(message: string): never {
  throw new MemorySpaceInputError("missing_required_parameter", message);
}

function routeSuccess<T>(requestId: string, data: T): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function routeFailure(
  status: number,
  requestId: string,
  input: { reason_code: string; message: string },
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error: {
        code:
          status === 404
            ? "not_found"
            : status === 405
              ? "method_not_allowed"
              : "invalid_input",
        reason_code: input.reason_code,
        message: input.message,
        retryable: false,
      },
      meta: { request_id: requestId, schema_version: 1 },
    },
  };
}

function toolResult<T extends MemorySpaceToolDetails>(
  details: T,
): BrainToolResult<T> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
  };
}

function toolReasonCode(
  error: unknown,
): Extract<MemorySpaceToolDetails, { ok: false }>["reason_code"] {
  if (error instanceof MemorySpaceInputError) {
    if (error.reasonCode === "memory_space_records_unsupported") {
      return "memory_space_records_unsupported";
    }
    if (error.reasonCode === "invalid_memory_space_key") {
      return "invalid_memory_space_key";
    }
    if (error.reasonCode === "missing_required_parameter") {
      return "missing_required_parameter";
    }
    if (error.reasonCode === "target_id_required") {
      return "target_id_required";
    }
  }
  return "invalid_memory_space_input";
}

class MemorySpaceInputError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
    this.name = "MemorySpaceInputError";
  }
}
