import type { AdminRouteResult } from "./admin-diagnostics-api.js";

export interface ServiceRouteError {
  code:
    | "unauthorized"
    | "forbidden"
    | "method_not_allowed"
    | "not_found"
    | "invalid_input"
    | "failed_precondition"
    | "conflict"
    | "internal_error";
  reason_code: string;
  message: string;
  retryable: boolean;
}

export function failure(
  status: number,
  requestIdValue: string,
  error: ServiceRouteError,
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error,
      meta: { request_id: requestIdValue, schema_version: 1 },
    },
  };
}

export function successRoute<T>(
  requestIdValue: string,
  data: T,
): AdminRouteResult<T> {
  return {
    status: 200,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data,
      meta: { request_id: requestIdValue, schema_version: 1 },
    },
  };
}

export function readOnlyMethod(
  method: string | undefined,
  requestIdValue: string,
  reasonCode: string,
  message: string,
): AdminRouteResult | undefined {
  if ((method ?? "GET").toUpperCase() === "GET") return undefined;
  return failure(405, requestIdValue, {
    code: "method_not_allowed",
    reason_code: reasonCode,
    message,
    retryable: false,
  });
}
