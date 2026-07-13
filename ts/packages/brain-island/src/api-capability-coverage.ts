import { API_CAPABILITIES } from "./api-command-registry.js";
import {
  SERVICE_API_ROUTE_TABLE,
  matchServiceApiRoute,
  type ServiceApiRouteAuthPhase,
  type ServiceApiRouteId,
} from "./service-route-table.js";

export interface ApiCapabilityRouteCoverage {
  capability_id: string;
  method: string;
  path_template: string;
  route_id: ServiceApiRouteId;
  auth_phase: ServiceApiRouteAuthPhase;
}

export interface ServiceRouteCatalogExemption {
  route_id: ServiceApiRouteId;
  reason: string;
}

export interface ApiCapabilityCoverageInventory {
  capability_routes: ApiCapabilityRouteCoverage[];
  route_exemptions: ServiceRouteCatalogExemption[];
}

export const SERVICE_ROUTE_CATALOG_EXEMPTIONS = [
  {
    route_id: "browser.cors",
    reason:
      "preflight route only; cataloging each CORS path would duplicate browser routes",
  },
  {
    route_id: "debug",
    reason:
      "debug routes are intentionally omitted from public capability discovery",
  },
  {
    route_id: "admin.mcp.catalog",
    reason:
      "legacy MCP catalog route is route-table visible but not a Rusty View capability surface",
  },
  {
    route_id: "admin.model_providers",
    reason:
      "model-provider admin capability metadata remains in its dedicated contract until it joins the shared catalog",
  },
] as const satisfies readonly ServiceRouteCatalogExemption[];

export function apiCapabilityCoverageInventory(): ApiCapabilityCoverageInventory {
  const capabilityRoutes = API_CAPABILITIES.filter(
    (capability) => capability.public,
  ).map<ApiCapabilityRouteCoverage>((capability) => {
    const authPhase = capabilityAuthPhase(capability.auth);
    const samplePath = samplePathTemplate(capability.path_template);
    const route = matchServiceApiRoute(samplePath, authPhase);
    if (!route) {
      throw new Error(
        `public API capability ${capability.id} has no ${authPhase} service route for ${capability.method} ${samplePath}`,
      );
    }
    return {
      capability_id: capability.id,
      method: capability.method,
      path_template: capability.path_template,
      route_id: route.id,
      auth_phase: authPhase,
    };
  });

  const coveredRoutes = new Set(
    capabilityRoutes.map((entry) => entry.route_id),
  );
  const exemptions = new Map<ServiceApiRouteId, ServiceRouteCatalogExemption>(
    SERVICE_ROUTE_CATALOG_EXEMPTIONS.map((entry) => [entry.route_id, entry]),
  );
  for (const route of SERVICE_API_ROUTE_TABLE) {
    const covered = coveredRoutes.has(route.id);
    const exemption = exemptions.get(route.id);
    if (!covered && !exemption) {
      throw new Error(
        `service route family ${route.id} has no public capability descriptor or catalog exemption`,
      );
    }
    if (covered && exemption) {
      throw new Error(
        `service route family ${route.id} has both capability coverage and stale exemption: ${exemption.reason}`,
      );
    }
  }

  const routeIds = new Set(SERVICE_API_ROUTE_TABLE.map((route) => route.id));
  for (const exemption of SERVICE_ROUTE_CATALOG_EXEMPTIONS) {
    if (!routeIds.has(exemption.route_id)) {
      throw new Error(
        `catalog exemption references missing service route family ${exemption.route_id}`,
      );
    }
  }

  return {
    capability_routes: capabilityRoutes,
    route_exemptions: SERVICE_ROUTE_CATALOG_EXEMPTIONS.map((entry) => ({
      ...entry,
    })),
  };
}

function capabilityAuthPhase(auth: "none" | "chat" | "admin") {
  return auth === "none" ? "before_auth" : "after_auth";
}

function samplePathTemplate(pathTemplate: string): string {
  return pathTemplate.replace(/\{[^}]+\}/g, "sample");
}
