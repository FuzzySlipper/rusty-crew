import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { buildBuiltInToolCatalog } from "./tool-registry.js";
import { readOnlyMethod, successRoute } from "./service-route-results.js";

export interface AdminToolCatalogRouteRequest {
  method?: string;
  requestId: string;
}

export async function handleAdminToolsCatalogRequest(
  request: AdminToolCatalogRouteRequest,
): Promise<AdminRouteResult> {
  const methodFailure = readOnlyMethod(
    request.method,
    request.requestId,
    "tool_catalog_read_only",
    "built-in tool catalog routes only support GET",
  );
  if (methodFailure) return methodFailure;

  return successRoute(request.requestId, buildBuiltInToolCatalog());
}
