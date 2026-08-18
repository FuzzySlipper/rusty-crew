import type {
  AgentMessageDeliveryReceipt,
  AgentRouteResolution,
} from "@rusty-crew/contracts";
import type {
  RustyCrewDeploymentRole,
  RustyCrewReviewGithubGateBypassPolicy,
  RustyCrewReviewDenAuthorityConfig,
} from "./service-config.js";
import { validateReviewDenAuthorityConfig } from "./service-config.js";
import type { ReviewDenAuthorityDiagnostics } from "./service-review-den-authority.js";
import {
  composedReviewPipeline,
  reviewOperatorConfigRevision,
  reviewOperatorConfigReadback,
  type StaleReviewTask,
} from "./service-review-operator.js";
import {
  failure,
  successRoute,
  type ServiceRouteResult,
} from "./service-route-results.js";

const PREFIX = "/v1/admin/review-operator";

export interface ReviewOperatorRouteContext {
  deploymentRole: RustyCrewDeploymentRole;
  githubGateBypass?(): RustyCrewReviewGithubGateBypassPolicy;
  authority(): RustyCrewReviewDenAuthorityConfig | undefined;
  diagnostics(): ReviewDenAuthorityDiagnostics;
  refreshDiagnostics(): Promise<ReviewDenAuthorityDiagnostics>;
  resolveReviewer(): Promise<
    import("@rusty-crew/contracts").AgentRouteResolution
  >;
  readRuntimeConfigFile(): Promise<{ value: Record<string, unknown> }>;
  writeRuntimeConfigFile(value: Record<string, unknown>): Promise<void>;
  applyRuntimeConfigFromDisk(): Promise<unknown>;
  reconcileSubmissions?(): Promise<void>;
  withRuntimeConfigMutation<T>(mutation: () => Promise<T>): Promise<T>;
  pipeline(input: {
    projectId: string;
    limit: number;
    offset: number;
  }): ReturnType<typeof composedReviewPipeline>;
  staleTasks?(input: {
    projectIds: readonly string[];
    staleMs: number;
  }): Promise<StaleReviewTask[]>;
  promptReviewer(input: {
    taskId: number;
    ttlMs: number;
    correlationId?: string;
    idempotencyKey?: string;
  }): Promise<AgentMessageDeliveryReceipt>;
}

export function isReviewOperatorRoute(pathname: string): boolean {
  return pathname === PREFIX || pathname.startsWith(`${PREFIX}/`);
}

export async function handleReviewOperatorRequest(
  input: {
    method: string;
    url: URL;
    requestId: string;
    body?: unknown;
  },
  context: ReviewOperatorRouteContext,
): Promise<ServiceRouteResult> {
  try {
    assertExpectedRole(
      input.url.searchParams.get("expectedDeploymentRole"),
      context.deploymentRole,
    );
    if (input.url.pathname === `${PREFIX}/github-gate-bypass`) {
      if (input.method === "GET") {
        return successRoute(
          input.requestId,
          requiredGithubGateBypass(context)(),
        );
      }
      if (input.method !== "PATCH") return methodNotAllowed(input.requestId);
      const body = recordBody(input.body);
      assertExpectedRole(
        optionalString(body.expectedDeploymentRole),
        context.deploymentRole,
      );
      const expectedRevision = requiredString(
        body.expectedConfigRevision,
        "expectedConfigRevision",
      );
      if (typeof body.enabled !== "boolean") {
        throw new Error("enabled must be a boolean");
      }
      const reason = optionalString(body.reason);
      if (body.enabled && reason === undefined) {
        throw new Error(
          "reason is required when enabling the GitHub gate bypass",
        );
      }
      return await context.withRuntimeConfigMutation(async () => {
        const current = requiredGithubGateBypass(context)();
        if (current.configRevision !== expectedRevision) {
          throw new ReviewOperatorConflictError(
            "review_github_gate_bypass_revision_conflict",
            "review GitHub gate bypass config revision conflict",
          );
        }
        const runtimeFile = await context.readRuntimeConfigFile();
        const previousValue = structuredClone(runtimeFile.value);
        runtimeFile.value.reviewGithubGateBypass = {
          enabled: body.enabled,
          deploymentRole: context.deploymentRole,
          ...(reason === undefined ? {} : { reason }),
        };
        await context.writeRuntimeConfigFile(runtimeFile.value);
        let applyResult: unknown;
        try {
          applyResult = await context.applyRuntimeConfigFromDisk();
          await context.reconcileSubmissions?.().catch(() => undefined);
        } catch (error) {
          await context.writeRuntimeConfigFile(previousValue);
          await context.applyRuntimeConfigFromDisk().catch(() => undefined);
          throw error;
        }
        return successRoute(input.requestId, {
          status: "updated",
          config: requiredGithubGateBypass(context)(),
          applyResult,
        });
      });
    }
    if (input.url.pathname === `${PREFIX}/config`) {
      if (input.method === "GET") {
        const diagnostics = await context.refreshDiagnostics();
        const reviewerRoute = await resolveReviewerRoute(context);
        return successRoute(
          input.requestId,
          reviewOperatorConfigReadback({
            deploymentRole: context.deploymentRole,
            authority: context.authority(),
            diagnostics,
            reviewerRoute,
          }),
        );
      }
      if (input.method !== "PATCH") return methodNotAllowed(input.requestId);
      const body = recordBody(input.body);
      assertExpectedRole(
        optionalString(body.expectedDeploymentRole),
        context.deploymentRole,
      );
      rejectSecretFields(body);
      const expectedRevision = requiredString(
        body.expectedConfigRevision,
        "expectedConfigRevision",
      );
      return await context.withRuntimeConfigMutation(async () => {
        const actualRevision = reviewOperatorConfigRevision(
          context.authority(),
        );
        if (expectedRevision !== actualRevision) {
          throw new ReviewOperatorConflictError(
            "review_den_authority_revision_conflict",
            "review Den authority config revision conflict",
          );
        }
        const runtimeFile = await context.readRuntimeConfigFile();
        const previousValue = structuredClone(runtimeFile.value);
        if (body.enabled === false) {
          runtimeFile.value.reviewDenAuthority = null;
        } else {
          const authority: RustyCrewReviewDenAuthorityConfig = {
            authorityId: requiredString(body.authorityId, "authorityId"),
            endpointRef: requiredString(body.endpointRef, "endpointRef"),
            serverName: "den",
            toolProfileKey: "direct",
            auditIdentity:
              optionalString(body.auditIdentity) ?? "rusty-crew-review-service",
          };
          validateReviewDenAuthorityConfig(authority);
          runtimeFile.value.reviewDenAuthority = authority;
        }
        await context.writeRuntimeConfigFile(runtimeFile.value);
        let applyResult: unknown;
        try {
          applyResult = await context.applyRuntimeConfigFromDisk();
        } catch (error) {
          await context.writeRuntimeConfigFile(previousValue);
          await context.applyRuntimeConfigFromDisk().catch(() => undefined);
          throw error;
        }
        const diagnostics = await context.refreshDiagnostics();
        const reviewerRoute = await resolveReviewerRoute(context);
        return successRoute(input.requestId, {
          status: "updated",
          config: reviewOperatorConfigReadback({
            deploymentRole: context.deploymentRole,
            authority: context.authority(),
            diagnostics,
            reviewerRoute,
          }),
          applyResult,
        });
      });
    }

    if (input.url.pathname === `${PREFIX}/pipeline`) {
      if (input.method !== "GET") return methodNotAllowed(input.requestId);
      const projectId = requiredString(
        input.url.searchParams.get("projectId"),
        "projectId",
      );
      const limit = boundedInteger(
        input.url.searchParams.get("limit"),
        50,
        1,
        100,
        "limit",
      );
      const offset = boundedInteger(
        input.url.searchParams.get("offset"),
        0,
        0,
        Number.MAX_SAFE_INTEGER,
        "offset",
      );
      return successRoute(
        input.requestId,
        await context.pipeline({ projectId, limit, offset }),
      );
    }

    if (input.url.pathname === `${PREFIX}/stale-review-tasks`) {
      if (input.method !== "GET") return methodNotAllowed(input.requestId);
      const projectIds = input.url.searchParams
        .getAll("projectId")
        .map((value) => value.trim())
        .filter(Boolean);
      const staleMs = boundedInteger(
        input.url.searchParams.get("staleMs"),
        300_000,
        0,
        Number.MAX_SAFE_INTEGER,
        "staleMs",
      );
      return successRoute(
        input.requestId,
        await requiredStaleTasks(context)({ projectIds, staleMs }),
      );
    }

    const taskMatch = input.url.pathname.match(
      /^\/v1\/admin\/review-operator\/tasks\/(\d+)\/prompt-reviewer$/,
    );
    if (taskMatch !== null) {
      if (input.method !== "POST") return methodNotAllowed(input.requestId);
      const body = recordBody(input.body);
      assertExpectedRole(
        optionalString(body.expectedDeploymentRole),
        context.deploymentRole,
      );
      const taskId = Number(taskMatch[1]);
      const reviewerRoute = await resolveReviewerRoute(context);
      if (!reviewerRoute.routable || reviewerRoute.resolvedTarget == null) {
        throw new ReviewOperatorConflictError(
          reviewerRoute.reasonCode ?? "reviewer_route_unavailable",
          "@reviewer does not resolve to a routable session",
        );
      }
      const ttlMs = boundedInteger(
        body.ttlMs,
        300_000,
        1_000,
        900_000,
        "ttlMs",
      );
      const receipt = await context.promptReviewer({
        taskId,
        ttlMs,
        ...(optionalString(body.correlationId) === undefined
          ? {}
          : { correlationId: optionalString(body.correlationId) }),
        ...(optionalString(body.idempotencyKey) === undefined
          ? {}
          : { idempotencyKey: optionalString(body.idempotencyKey) }),
      });
      return successRoute(input.requestId, {
        deploymentRole: context.deploymentRole,
        command: `review ${taskId}`,
        target: "@reviewer",
        receipt,
      });
    }

    return failure(404, input.requestId, {
      code: "not_found",
      reason_code: "unknown_review_operator_route",
      message: `unknown review operator route ${input.url.pathname}`,
      retryable: false,
    });
  } catch (error) {
    const conflict = error instanceof ReviewOperatorConflictError;
    return failure(conflict ? 409 : 400, input.requestId, {
      code: conflict ? "conflict" : "invalid_input",
      reason_code: conflict
        ? error.reasonCode
        : "review_operator_request_failed",
      message: error instanceof Error ? error.message : String(error),
      retryable: false,
    });
  }
}

function requiredStaleTasks(
  context: ReviewOperatorRouteContext,
): NonNullable<ReviewOperatorRouteContext["staleTasks"]> {
  if (context.staleTasks === undefined) {
    throw new Error("stale review task query is unavailable");
  }
  return context.staleTasks;
}

function requiredGithubGateBypass(
  context: ReviewOperatorRouteContext,
): NonNullable<ReviewOperatorRouteContext["githubGateBypass"]> {
  if (context.githubGateBypass === undefined) {
    throw new Error("review GitHub gate bypass config is unavailable");
  }
  return context.githubGateBypass;
}

class ReviewOperatorConflictError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
  }
}

async function resolveReviewerRoute(
  context: ReviewOperatorRouteContext,
): Promise<AgentRouteResolution> {
  try {
    return await context.resolveReviewer();
  } catch (error) {
    return {
      address: "@reviewer",
      routable: false,
      reasonCode:
        error instanceof Error &&
        error.message.includes("agent_route_not_found")
          ? "agent_route_not_found"
          : "reviewer_route_resolution_failed",
    };
  }
}

function assertExpectedRole(
  value: string | null | undefined,
  actual: RustyCrewDeploymentRole,
): void {
  if (value === undefined || value === null || value === "") return;
  if (value !== actual)
    throw new Error(`expected ${value} deployment but reached ${actual}`);
}

function rejectSecretFields(body: Record<string, unknown>): void {
  for (const field of ["bearerToken", "bearer_token", "token", "credential"]) {
    if (Object.hasOwn(body, field))
      throw new Error(
        `${field} is server-managed and cannot be written through this API`,
      );
  }
}

function methodNotAllowed(requestId: string): ServiceRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "review_operator_method_not_allowed",
    message: "review operator route does not support this method",
    retryable: false,
  });
}

function boundedInteger(
  value: unknown,
  fallback: number,
  minimum: number,
  maximum: number,
  name: string,
): number {
  if (value === undefined || value === null || value === "") return fallback;
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < minimum || parsed > maximum) {
    throw new Error(`${name} must be an integer from ${minimum} to ${maximum}`);
  }
  return parsed;
}

function recordBody(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value))
    return {};
  return value as Record<string, unknown>;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function requiredString(value: unknown, name: string): string {
  const result = optionalString(value);
  if (result === undefined) throw new Error(`${name} is required`);
  return result;
}
