import { isChatRoute } from "./service-chat-stream-routes.js";
import { isProfileRegistryWriteRoute } from "./service-profile-registry-routes.js";
import { isRoleplayBrowserRoute } from "./service-roleplay-routes.js";

export type ServiceApiRouteId =
  | "admin.healthz"
  | "browser.cors"
  | "admin.control"
  | "chat"
  | "debug"
  | "admin.scheduler"
  | "admin.mcp.catalog"
  | "admin.mcp.servers"
  | "admin.tools.catalog"
  | "admin.brain_catalog"
  | "admin.context_strategies"
  | "admin.local_tool_profiles"
  | "roleplay"
  | "admin.storage"
  | "admin.model_providers"
  | "admin.profile_registry.write"
  | "admin.memory"
  | "admin.diagnostics";

export type ServiceApiRouteAuthPhase =
  | "before_auth"
  | "cors_preflight"
  | "after_auth";

export interface ServiceApiRouteDescriptor {
  readonly id: ServiceApiRouteId;
  readonly order: number;
  readonly authPhase: ServiceApiRouteAuthPhase;
  readonly summary: string;
  matches(pathname: string): boolean;
}

export const SERVICE_API_ROUTE_TABLE: readonly ServiceApiRouteDescriptor[] = [
  route(
    "admin.healthz",
    10,
    "before_auth",
    "Admin health check",
    (path) => path === "/v1/admin/healthz",
  ),
  route(
    "browser.cors",
    20,
    "cors_preflight",
    "Browser-facing CORS preflight for chat and roleplay routes",
    isBrowserCorsRoute,
  ),
  route(
    "admin.control",
    100,
    "after_auth",
    "Admin control command routes",
    (path) => path.startsWith("/v1/admin/control/"),
  ),
  route("chat", 110, "after_auth", "Rusty View chat routes", isChatRoute),
  route(
    "debug",
    120,
    "after_auth",
    "Direct debug and provider-request debug routes",
    (path) => path.startsWith("/v1/debug/"),
  ),
  route("admin.scheduler", 130, "after_auth", "Scheduler read routes", (path) =>
    path.startsWith("/v1/admin/scheduler/"),
  ),
  route(
    "admin.mcp.catalog",
    140,
    "after_auth",
    "MCP catalog read route",
    (path) => path === "/v1/admin/mcp/catalog",
  ),
  route(
    "admin.mcp.servers",
    150,
    "after_auth",
    "MCP server registry routes",
    (path) =>
      path === "/v1/admin/mcp/servers" ||
      path.startsWith("/v1/admin/mcp/servers/"),
  ),
  route(
    "admin.tools.catalog",
    160,
    "after_auth",
    "Built-in tools catalog routes",
    (path) =>
      path === "/v1/admin/tools/catalog" ||
      path === "/v1/admin/tool-policy/catalog",
  ),
  route(
    "admin.brain_catalog",
    165,
    "after_auth",
    "Rust brain catalog route",
    (path) => path === "/v1/admin/brains/catalog",
  ),
  route(
    "admin.context_strategies",
    170,
    "after_auth",
    "Context strategy catalog route",
    (path) => path === "/v1/admin/context-strategies",
  ),
  route(
    "admin.local_tool_profiles",
    180,
    "after_auth",
    "Local tool profile registry routes",
    (path) =>
      path === "/v1/admin/local-tool-profiles" ||
      path.startsWith("/v1/admin/local-tool-profiles/"),
  ),
  route(
    "roleplay",
    190,
    "after_auth",
    "Roleplay browser/admin routes",
    isRoleplayBrowserRoute,
  ),
  route("admin.storage", 200, "after_auth", "Storage query routes", (path) =>
    path.startsWith("/v1/admin/storage/"),
  ),
  route(
    "admin.model_providers",
    210,
    "after_auth",
    "Model provider admin routes",
    (path) => path.startsWith("/v1/admin/model-providers"),
  ),
  route(
    "admin.profile_registry.write",
    220,
    "after_auth",
    "Profile registry write routes",
    isProfileRegistryWriteRoute,
  ),
  route("admin.memory", 230, "after_auth", "Memory admin routes", (path) =>
    path.startsWith("/v1/admin/memory/"),
  ),
  route(
    "admin.diagnostics",
    900,
    "after_auth",
    "Admin diagnostics fallback routes",
    (path) => path.startsWith("/v1/admin/"),
  ),
].sort((left, right) => left.order - right.order);

export function matchServiceApiRoute(
  pathname: string,
  authPhase?: ServiceApiRouteAuthPhase,
): ServiceApiRouteDescriptor | undefined {
  return SERVICE_API_ROUTE_TABLE.find(
    (descriptor) =>
      (authPhase === undefined || descriptor.authPhase === authPhase) &&
      descriptor.matches(pathname),
  );
}

export function isBrowserCorsRoute(pathname: string): boolean {
  return isChatRoute(pathname) || isRoleplayBrowserRoute(pathname);
}

function route(
  id: ServiceApiRouteId,
  order: number,
  authPhase: ServiceApiRouteAuthPhase,
  summary: string,
  matches: (pathname: string) => boolean,
): ServiceApiRouteDescriptor {
  return { id, order, authPhase, summary, matches };
}
