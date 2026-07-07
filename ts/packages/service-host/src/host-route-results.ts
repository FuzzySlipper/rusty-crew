import type { ServerResponse } from "node:http";

export interface HostRawRouteResult {
  kind: "raw";
  write(response: ServerResponse): void;
}

export type HostRouteResult =
  | HostRawRouteResult
  | {
      status: number;
      headers: Record<string, string>;
      body: string | HostJsonEnvelope;
    };

export interface HostJsonEnvelope {
  ok: boolean;
  error?: HostRouteError;
  meta: {
    request_id: string;
    schema_version: number;
  };
}

export interface HostRouteError {
  code: "forbidden" | "method_not_allowed" | "not_found" | "internal_error";
  reason_code: string;
  message: string;
  retryable: boolean;
}

export function hostFailure(
  status: number,
  requestIdValue: string,
  error: HostRouteError,
): HostRouteResult {
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

export function writeHostRouteResult(
  response: ServerResponse,
  result: HostRouteResult,
): void {
  if (isHostRawRouteResult(result)) {
    result.write(response);
    return;
  }
  for (const [name, value] of Object.entries(result.headers)) {
    response.setHeader(name, value);
  }
  response.statusCode = result.status;
  response.end(
    typeof result.body === "string" ? result.body : JSON.stringify(result.body),
  );
}

export function isHostRawRouteResult(
  result: HostRouteResult,
): result is HostRawRouteResult {
  return "kind" in result && result.kind === "raw";
}
