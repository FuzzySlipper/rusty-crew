import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { readOnlyMethod, successRoute } from "./service-route-results.js";

export async function handleAdminBrainCatalogRequest(
  request: { method?: string; requestId: string },
  bridge: Pick<NativeBridgeModule, "brainCatalog">,
): Promise<AdminRouteResult> {
  const methodFailure = readOnlyMethod(
    request.method,
    request.requestId,
    "brain_catalog_read_only",
    "brain catalog routes only support GET",
  );
  if (methodFailure) return methodFailure;
  return successRoute(request.requestId, await bridge.brainCatalog());
}
