import type {
  NativeProfileRegistryRecord,
  NativeProfileRegistryWrite,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import { failure, successRoute } from "./service-route-results.js";

export interface ProfileRegistryWriteRoute {
  profileId: string;
  kind: "update" | "lifecycle" | "prompt" | "runtime-config";
  mode: "plan" | "apply";
}

export interface ProfileRegistryRoutePlan {
  ok: boolean;
  expectedRevision: number;
  nextWrite: NativeProfileRegistryWrite;
}

export interface ProfileRegistryRouteRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId: string;
}

export interface ProfileRegistryWriteRouteContext {
  planRegistryWrite(
    route: ProfileRegistryWriteRoute,
    body: unknown,
  ): Promise<ProfileRegistryRoutePlan>;
  planRuntimeConfigWrite(
    route: ProfileRegistryWriteRoute,
    body: unknown,
  ): Promise<ProfileRegistryRoutePlan>;
  updateProfileRegistryRecord(input: {
    write: NativeProfileRegistryWrite;
    expectedRevision: number;
  }): Promise<NativeProfileRegistryRecord>;
  applyLifecycleEffects(record: NativeProfileRegistryRecord): Promise<unknown>;
  applyRuntimeConfigEffects(
    record: NativeProfileRegistryRecord,
    plan: ProfileRegistryRoutePlan,
  ): Promise<unknown>;
}

export async function handleProfileRegistryWriteRequest(
  request: ProfileRegistryRouteRequest,
  context: ProfileRegistryWriteRouteContext,
): Promise<AdminRouteResult> {
  const method = request.method.toUpperCase();
  if (method !== "POST" && method !== "PATCH") {
    return failure(405, request.requestId, {
      code: "method_not_allowed",
      reason_code: "profile_registry_write_requires_post_or_patch",
      message: "profile registry write routes support POST or PATCH",
      retryable: false,
    });
  }
  const route = parseProfileRegistryWriteRoute(new URL(request.url).pathname);
  if (route === undefined) {
    return failure(404, request.requestId, {
      code: "not_found",
      reason_code: "unknown_profile_registry_write_route",
      message: "unknown profile registry write route",
      retryable: false,
    });
  }
  let plan: ProfileRegistryRoutePlan;
  try {
    plan =
      route.kind === "runtime-config"
        ? await context.planRuntimeConfigWrite(route, request.body)
        : await context.planRegistryWrite(route, request.body);
  } catch (error) {
    if (isMissingProfileRegistryRecord(error)) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "profile_registry_record_missing",
        message: (error as Error).message,
        retryable: false,
      });
    }
    throw error;
  }
  if (route.mode === "plan") return successRoute(request.requestId, plan);
  if (!plan.ok) return successRoute(request.requestId, plan);
  const updated = await context.updateProfileRegistryRecord({
    write: plan.nextWrite,
    expectedRevision: plan.expectedRevision,
  });
  const effects =
    route.kind === "lifecycle"
      ? await context.applyLifecycleEffects(updated)
      : route.kind === "runtime-config"
        ? await context.applyRuntimeConfigEffects(updated, plan)
        : undefined;
  return successRoute(request.requestId, {
    ...plan,
    applied: true,
    record: updated,
    effects,
  });
}

export function parseProfileRegistryWriteRoute(
  pathname: string,
): ProfileRegistryWriteRoute | undefined {
  const parts = pathname.split("/").filter(Boolean);
  if (
    parts.length !== 7 ||
    parts[0] !== "v1" ||
    parts[1] !== "admin" ||
    parts[2] !== "profiles" ||
    parts[3] !== "registry"
  ) {
    return undefined;
  }
  const kind = parts[5];
  const mode = parts[6];
  if (
    (kind !== "update" &&
      kind !== "lifecycle" &&
      kind !== "prompt" &&
      kind !== "runtime-config") ||
    (mode !== "plan" && mode !== "apply")
  ) {
    return undefined;
  }
  return {
    profileId: decodeURIComponent(parts[4] ?? ""),
    kind: kind === "runtime-config" ? "runtime-config" : kind,
    mode,
  };
}

export function isProfileRegistryWriteRoute(pathname: string): boolean {
  return (
    pathname.startsWith("/v1/admin/profiles/registry/") &&
    (pathname.endsWith("/update/plan") ||
      pathname.endsWith("/update/apply") ||
      pathname.endsWith("/lifecycle/plan") ||
      pathname.endsWith("/lifecycle/apply") ||
      pathname.endsWith("/prompt/plan") ||
      pathname.endsWith("/prompt/apply") ||
      pathname.endsWith("/runtime-config/plan") ||
      pathname.endsWith("/runtime-config/apply"))
  );
}

function isMissingProfileRegistryRecord(error: unknown): boolean {
  return (
    error instanceof Error &&
    error.message.includes(
      "was not found; create or import a DB-backed profile",
    )
  );
}
