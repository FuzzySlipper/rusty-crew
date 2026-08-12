import type { AgentMessageDeliveryReceipt } from "@rusty-crew/contracts";
import type {
  RustyCrewDeploymentRole,
  RustyCrewReviewDenAuthorityConfig,
} from "./service-config.js";
import { validateReviewDenAuthorityConfig } from "./service-config.js";
import type { ReviewDenAuthorityDiagnostics } from "./service-review-den-authority.js";
import {
  composedReviewPipeline,
  reviewOperatorConfigReadback,
} from "./service-review-operator.js";
import {
  failure,
  successRoute,
  type ServiceRouteResult,
} from "./service-route-results.js";

const PREFIX = "/v1/admin/review-operator";

export interface ReviewOperatorRouteContext {
  deploymentRole: RustyCrewDeploymentRole;
  authority(): RustyCrewReviewDenAuthorityConfig | undefined;
  diagnostics(): ReviewDenAuthorityDiagnostics;
  refreshDiagnostics(): Promise<ReviewDenAuthorityDiagnostics>;
  readRuntimeConfigFile(): Promise<{ value: Record<string, unknown> }>;
  writeRuntimeConfigFile(value: Record<string, unknown>): Promise<void>;
  applyRuntimeConfigFromDisk(): Promise<unknown>;
  withRuntimeConfigMutation<T>(mutation: () => Promise<T>): Promise<T>;
  pipeline(input: {
    projectId: string;
    limit: number;
    offset: number;
  }): ReturnType<typeof composedReviewPipeline>;
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
    if (input.url.pathname === `${PREFIX}/config`) {
      if (input.method === "GET") {
        const diagnostics = await context.refreshDiagnostics();
        return successRoute(
          input.requestId,
          reviewOperatorConfigReadback({
            deploymentRole: context.deploymentRole,
            authority: context.authority(),
            diagnostics,
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
      return context.withRuntimeConfigMutation(async () => {
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
        return successRoute(input.requestId, {
          status: "updated",
          config: reviewOperatorConfigReadback({
            deploymentRole: context.deploymentRole,
            authority: context.authority(),
            diagnostics,
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
    return failure(400, input.requestId, {
      code: "invalid_input",
      reason_code: "review_operator_request_failed",
      message: error instanceof Error ? error.message : String(error),
      retryable: false,
    });
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
