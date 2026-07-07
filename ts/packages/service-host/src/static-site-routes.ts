import { createReadStream, existsSync } from "node:fs";
import { stat } from "node:fs/promises";
import { basename, extname, join, relative, resolve, sep } from "node:path";

import {
  hostFailure,
  type HostRawRouteResult,
  type HostRouteResult,
} from "./host-route-results.js";

export interface StaticSiteRouteRequest {
  method?: string;
  pathname: string;
  requestId: string;
}

export interface StaticSiteRouteContext {
  root: string | undefined;
}

export function staticSiteRootFromPaths(paths: {
  staticDir?: string;
  dataDir: string;
}): string | undefined {
  return paths.staticDir ?? join(paths.dataDir, "site");
}

export function staticServingEnabled(root: string | undefined): boolean {
  return root !== undefined && existsSync(root);
}

export async function handleStaticSiteRequest(
  request: StaticSiteRouteRequest,
  context: StaticSiteRouteContext,
): Promise<HostRouteResult> {
  if ((request.method ?? "GET").toUpperCase() !== "GET") {
    return hostFailure(405, request.requestId, {
      code: "method_not_allowed",
      reason_code: "static_method_not_allowed",
      message: "static files only support GET",
      retryable: false,
    });
  }
  const root = context.root;
  if (root === undefined) {
    return hostFailure(404, request.requestId, {
      code: "not_found",
      reason_code: "static_site_disabled",
      message: "static site serving is not configured",
      retryable: false,
    });
  }
  const candidate = resolveStaticSitePath(root, request.pathname);
  if (!candidate.ok) {
    return hostFailure(403, request.requestId, {
      code: "forbidden",
      reason_code: candidate.reasonCode,
      message: candidate.message,
      retryable: false,
    });
  }

  const filePath = await existingStaticFile(candidate.path);
  if (filePath !== undefined) return staticFileResponse(root, filePath);

  const indexPath = resolve(root, "index.html");
  if (await isReadableFile(indexPath)) {
    return staticFileResponse(root, indexPath);
  }

  return hostFailure(404, request.requestId, {
    code: "not_found",
    reason_code: "static_index_missing",
    message: `static site index.html was not found in ${root}`,
    retryable: false,
  });
}

export function resolveStaticSitePath(
  root: string,
  pathname: string,
):
  | { ok: true; path: string }
  | { ok: false; reasonCode: string; message: string } {
  let decodedSegments: string[];
  try {
    decodedSegments = pathname
      .split("/")
      .filter(Boolean)
      .map((segment) => decodeURIComponent(segment));
  } catch {
    return {
      ok: false,
      reasonCode: "static_path_invalid",
      message: "static path contains invalid percent encoding",
    };
  }

  if (
    decodedSegments.some(
      (segment) =>
        segment === "." || segment === ".." || segment.startsWith("."),
    )
  ) {
    return {
      ok: false,
      reasonCode: "static_path_forbidden",
      message: "static path contains a forbidden segment",
    };
  }

  const resolvedRoot = resolve(root);
  const resolvedPath =
    decodedSegments.length === 0
      ? resolve(resolvedRoot, "index.html")
      : resolve(resolvedRoot, ...decodedSegments);
  const relativePath = relative(resolvedRoot, resolvedPath);
  if (
    relativePath === ".." ||
    relativePath.startsWith(`..${sep}`) ||
    resolve(resolvedPath) === resolvedRoot
  ) {
    return {
      ok: false,
      reasonCode: "static_path_traversal",
      message: "static path escapes the configured static directory",
    };
  }
  return { ok: true, path: resolvedPath };
}

async function existingStaticFile(path: string): Promise<string | undefined> {
  if (await isReadableFile(path)) return path;
  const indexPath = resolve(path, "index.html");
  return (await isReadableFile(indexPath)) ? indexPath : undefined;
}

async function isReadableFile(path: string): Promise<boolean> {
  try {
    return (await stat(path)).isFile();
  } catch {
    return false;
  }
}

function staticFileResponse(
  root: string,
  filePath: string,
): HostRawRouteResult {
  return {
    kind: "raw",
    write(response) {
      response.statusCode = 200;
      response.setHeader("content-type", staticContentType(filePath));
      response.setHeader("cache-control", staticCacheControl(root, filePath));
      createReadStream(filePath).pipe(response);
    },
  };
}

export function staticContentType(path: string): string {
  switch (extname(path).toLowerCase()) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
      return "application/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json; charset=utf-8";
    case ".svg":
      return "image/svg+xml";
    case ".ico":
      return "image/x-icon";
    case ".png":
      return "image/png";
    case ".woff2":
      return "font/woff2";
    default:
      return "application/octet-stream";
  }
}

export function staticCacheControl(root: string, filePath: string): string {
  const relativePath = relative(root, filePath);
  if (relativePath === "index.html" || basename(filePath) === "index.html") {
    return "no-cache";
  }
  return /-[a-z0-9]{16,}\./i.test(basename(filePath))
    ? "public, max-age=31536000, immutable"
    : "no-cache";
}
