import type {
  ScheduledJobStatus,
  ScheduledRunStatus,
  ScheduledRunTrigger,
  SessionId,
} from "@rusty-crew/contracts";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  failure,
  readOnlyMethod,
  successRoute,
} from "./service-route-results.js";

export interface SchedulerReadRouteRequest {
  method?: string;
  url: URL;
  requestId: string;
}

export interface SchedulerReadRouteContext {
  listScheduledJobs(input: {
    status?: ScheduledJobStatus;
    jobKind?: string;
    limit?: number;
    offset?: number;
  }): Promise<unknown>;
  listScheduledRuns(input: {
    jobId?: string;
    status?: ScheduledRunStatus;
    trigger?: ScheduledRunTrigger;
    targetSessionId?: SessionId;
    limit?: number;
    offset?: number;
  }): Promise<unknown>;
}

export async function handleSchedulerReadRequest(
  request: SchedulerReadRouteRequest,
  context: SchedulerReadRouteContext,
): Promise<AdminRouteResult> {
  const methodFailure = readOnlyMethod(
    request.method,
    request.requestId,
    "read_only_route",
    "scheduler diagnostics routes only support GET",
  );
  if (methodFailure) return methodFailure;

  if (request.url.pathname === "/v1/admin/scheduler/jobs") {
    const status = scheduledJobStatusParam(
      request.url.searchParams.get("status"),
    );
    if (status === "invalid") {
      return invalidSchedulerFilter(request.requestId, "status");
    }
    const jobKind = stringParam(request.url, "jobKind");
    const jobs = await context.listScheduledJobs({
      ...(status === undefined ? {} : { status }),
      ...(jobKind === undefined ? {} : { jobKind }),
      ...pageParams(request.url),
    });
    return successRoute(request.requestId, { jobs });
  }

  if (request.url.pathname === "/v1/admin/scheduler/runs") {
    const status = scheduledRunStatusParam(
      request.url.searchParams.get("status"),
    );
    if (status === "invalid") {
      return invalidSchedulerFilter(request.requestId, "status");
    }
    const trigger = scheduledRunTriggerParam(
      request.url.searchParams.get("trigger"),
    );
    if (trigger === "invalid") {
      return invalidSchedulerFilter(request.requestId, "trigger");
    }
    const jobId = stringParam(request.url, "jobId");
    const targetSessionId = stringParam(request.url, "targetSessionId");
    const runs = await context.listScheduledRuns({
      ...(jobId === undefined ? {} : { jobId }),
      ...(status === undefined ? {} : { status }),
      ...(trigger === undefined ? {} : { trigger }),
      ...(targetSessionId === undefined
        ? {}
        : { targetSessionId: targetSessionId as SessionId }),
      ...pageParams(request.url),
    });
    return successRoute(request.requestId, { runs });
  }

  return failure(404, request.requestId, {
    code: "not_found",
    reason_code: "unknown_scheduler_diagnostics_route",
    message: `unknown scheduler diagnostics route ${request.url.pathname}`,
    retryable: false,
  });
}

function invalidSchedulerFilter(
  requestIdValue: string,
  key: string,
): AdminRouteResult {
  return failure(400, requestIdValue, {
    code: "invalid_input",
    reason_code: "invalid_scheduler_filter",
    message: `invalid scheduler ${key} filter`,
    retryable: false,
  });
}

export function scheduledJobStatusParam(
  value: string | null,
): ScheduledJobStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "active" || value === "paused" || value === "archived"
    ? value
    : "invalid";
}

export function scheduledRunStatusParam(
  value: string | null,
): ScheduledRunStatus | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "claimed" ||
    value === "completed" ||
    value === "skipped" ||
    value === "failed" ||
    value === "expired" ||
    value === "cancelled"
    ? value
    : "invalid";
}

export function scheduledRunTriggerParam(
  value: string | null,
): ScheduledRunTrigger | "invalid" | undefined {
  if (value === null || value.trim() === "") return undefined;
  return value === "due" || value === "manual" ? value : "invalid";
}

export function stringParam(url: URL, key: string): string | undefined {
  const value = url.searchParams.get(key);
  return value === null || value.trim() === "" ? undefined : value;
}

export function pageParams(url: URL): { limit?: number; offset?: number } {
  const limit = optionalInteger(url.searchParams.get("limit"));
  const offset = optionalInteger(url.searchParams.get("offset"));
  return {
    ...(limit === undefined ? {} : { limit }),
    ...(offset === undefined ? {} : { offset }),
  };
}

function optionalInteger(value: string | null): number | undefined {
  if (value === null || value.trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : undefined;
}
