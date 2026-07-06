import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import {
  LocalToolProfileError,
  type LocalToolProfileStore,
  type LocalToolProfileWrite,
} from "./local-tool-profiles.js";
import { failure, successRoute } from "./service-route-results.js";

export interface AdminLocalToolProfilesRouteRequest {
  method?: string;
  requestId: string;
  url: URL;
  readBody: () => Promise<unknown>;
}

export interface AdminLocalToolProfilesRouteContext {
  store: LocalToolProfileStore;
}

export async function handleAdminLocalToolProfilesRequest(
  request: AdminLocalToolProfilesRouteRequest,
  context: AdminLocalToolProfilesRouteContext,
): Promise<AdminRouteResult> {
  const method = (request.method ?? "GET").toUpperCase();
  const profileId = localToolProfileIdFromPath(request.url.pathname);
  try {
    if (request.url.pathname === "/v1/admin/local-tool-profiles") {
      if (method === "GET") {
        return successRoute(request.requestId, await context.store.list());
      }
      if (method === "POST") {
        const body = (await request.readBody()) as LocalToolProfileWrite;
        return successRoute(request.requestId, {
          profile: await context.store.create(body),
        });
      }
      return failure(405, request.requestId, {
        code: "method_not_allowed",
        reason_code: "local_tool_profiles_method_not_allowed",
        message: "local tool profile collection supports GET and POST",
        retryable: false,
      });
    }

    if (profileId === undefined) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "unknown_local_tool_profile_route",
        message: `unknown local tool profile route ${request.url.pathname}`,
        retryable: false,
      });
    }

    if (method === "GET") {
      const profile = await context.store.get(profileId);
      if (profile === undefined) {
        return failure(404, request.requestId, {
          code: "not_found",
          reason_code: "local_tool_profile_not_found",
          message: `local tool profile ${profileId} was not found`,
          retryable: false,
        });
      }
      return successRoute(request.requestId, { profile });
    }

    if (method === "PATCH") {
      const body = (await request.readBody()) as LocalToolProfileWrite;
      return successRoute(request.requestId, {
        profile: await context.store.update(profileId, body),
      });
    }

    if (method === "DELETE") {
      return successRoute(request.requestId, {
        profile: await context.store.delete(profileId),
        deleted: true,
      });
    }

    return failure(405, request.requestId, {
      code: "method_not_allowed",
      reason_code: "local_tool_profile_method_not_allowed",
      message: "local tool profile item routes support GET, PATCH, and DELETE",
      retryable: false,
    });
  } catch (error) {
    if (error instanceof LocalToolProfileError) {
      return failure(error.statusCode, request.requestId, {
        code: error.statusCode === 404 ? "not_found" : "invalid_input",
        reason_code: error.reasonCode,
        message: error.message,
        retryable: false,
      });
    }
    throw error;
  }
}

export function localToolProfileIdFromPath(
  pathname: string,
): string | undefined {
  const prefix = "/v1/admin/local-tool-profiles/";
  if (!pathname.startsWith(prefix)) return undefined;
  const rest = pathname.slice(prefix.length);
  if (!rest || rest.includes("/")) return undefined;
  return decodeURIComponent(rest);
}
