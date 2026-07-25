import type { SessionId, SessionState } from "@rusty-crew/contracts";
import {
  imageGenerationTool,
  type ImageGenerationRuntime,
} from "./image-generation.js";
import type { ToolMediaAttachmentStore } from "./tool-media-attachments.js";
import type { ServiceRouteResult } from "./service-route-results.js";
import { failure, successRoute } from "./service-route-results.js";

export interface ServiceImageGenerationRouteContext {
  runtime(): ImageGenerationRuntime;
  listSessions(): Promise<SessionState[]>;
  toolMediaAttachments: ToolMediaAttachmentStore;
}

export async function handleServiceImageGenerationRequest(
  request: {
    method: string;
    url: URL;
    body?: unknown;
    requestId: string;
  },
  context: ServiceImageGenerationRouteContext,
): Promise<ServiceRouteResult> {
  if (request.url.pathname === "/v1/admin/image-generation/presets") {
    if (request.method.toUpperCase() !== "GET") {
      return methodFailure(request.requestId);
    }
    const runtime = context.runtime();
    return successRoute(request.requestId, {
      presets: runtime.config.presets.map((preset) => ({
        id: preset.id,
        version: preset.version,
        provider_id: preset.providerId,
        defaults: preset.defaults,
        limits: preset.limits,
        styles: Object.keys(preset.styles).sort(),
      })),
    });
  }
  if (request.url.pathname === "/v1/admin/image-generation/generate") {
    if (request.method.toUpperCase() !== "POST") {
      return methodFailure(request.requestId);
    }
    const body = recordValue(request.body);
    const sessionId = stringValue(body.session_id);
    if (!sessionId) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "image_generation_session_required",
        message: "operator image generation requires session_id",
        retryable: false,
      });
    }
    const session = (await context.listSessions()).find(
      (candidate) => candidate.sessionId === sessionId,
    );
    if (!session) {
      return failure(404, request.requestId, {
        code: "not_found",
        reason_code: "image_generation_session_not_found",
        message: `session ${sessionId} was not found`,
        retryable: false,
      });
    }
    const tool = imageGenerationTool(context.runtime());
    let prepared: unknown;
    try {
      prepared = tool.prepareArguments?.(body) ?? body;
    } catch (error) {
      return failure(400, request.requestId, {
        code: "invalid_input",
        reason_code: "invalid_image_generation_request",
        message: error instanceof Error ? error.message : String(error),
        retryable: false,
      });
    }
    const callId = `operator-image:${request.requestId}`;
    const wakeId = `operator-image:${request.requestId}`;
    const result = await tool.execute(
      callId,
      prepared as never,
      AbortSignal.timeout(3_600_000),
    );
    const details = recordValue(result.details);
    if (details.ok === false) {
      return failure(502, request.requestId, {
        code: "failed_precondition",
        reason_code:
          stringValue(details.reasonCode) ?? "image_generation_failed",
        message:
          stringValue(details.message) ?? "image generation provider failed",
        retryable: details.retryable === true,
      });
    }
    const attachments = await context.toolMediaAttachments.persistImages({
      sessionId,
      wakeId,
      callId,
      toolName: tool.name,
      result,
    });
    return successRoute(request.requestId, {
      session_id: sessionId,
      wake_id: wakeId,
      attachments,
      details,
    });
  }
  return failure(404, request.requestId, {
    code: "not_found",
    reason_code: "image_generation_route_not_found",
    message: "image generation route not found",
    retryable: false,
  });
}

export function isServiceImageGenerationRoute(pathname: string): boolean {
  return pathname.startsWith("/v1/admin/image-generation/");
}

function methodFailure(requestId: string): ServiceRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "image_generation_method_not_allowed",
    message: "image generation route does not support this method",
    retryable: false,
  });
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value.trim()
    : undefined;
}
