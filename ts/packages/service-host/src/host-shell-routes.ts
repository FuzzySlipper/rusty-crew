import type { IncomingMessage } from "node:http";

import type { RustyCrewServiceConfig } from "@rusty-crew/brain-island";

import { adminPanelResponse, isAdminPanelRoute } from "./admin-panel-routes.js";
import type { HostRouteResult } from "./host-route-results.js";
import {
  handleStaticSiteRequest,
  staticServingEnabled,
  staticSiteRootFromPaths,
} from "./static-site-routes.js";

export async function handleHostShellRequest(
  request: IncomingMessage,
  config: RustyCrewServiceConfig,
): Promise<HostRouteResult | undefined> {
  const url = new URL(request.url ?? "/", "http://rusty-crew.local");
  const staticSiteRoot = staticSiteRootFromPaths(config.paths);
  if (isAdminPanelRoute(url.pathname, staticServingEnabled(staticSiteRoot))) {
    return adminPanelResponse(config.admin.authMode !== "none");
  }

  if (
    !url.pathname.startsWith("/v1/") &&
    staticServingEnabled(staticSiteRoot)
  ) {
    return handleStaticSiteRequest(
      {
        method: request.method,
        pathname: url.pathname,
        requestId: requestId(request),
      },
      { root: staticSiteRoot },
    );
  }

  return undefined;
}

export function requestId(request: IncomingMessage): string {
  const value = request.headers["x-request-id"];
  return typeof value === "string" && value.trim()
    ? value.trim()
    : `req_${Date.now()}`;
}
