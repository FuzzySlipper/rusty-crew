import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { contextStrategyCatalog } from "./context-strategy.js";
import { readOnlyMethod, successRoute } from "./service-route-results.js";

export interface AdminContextStrategyRouteRequest {
  method?: string;
  requestId: string;
}

export async function handleAdminContextStrategiesRequest(
  request: AdminContextStrategyRouteRequest,
): Promise<AdminRouteResult> {
  const methodFailure = readOnlyMethod(
    request.method,
    request.requestId,
    "context_strategy_catalog_read_only",
    "context strategy catalog routes only support GET",
  );
  if (methodFailure) return methodFailure;

  return successRoute(request.requestId, contextStrategyCatalog());
}
