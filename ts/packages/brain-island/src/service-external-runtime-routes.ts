import type { IncomingMessage, ServerResponse } from "node:http";
import { randomUUID } from "node:crypto";

import type {
  DenRuntimeReference,
  ExternalAgentBinding,
  ExternalAgentSessionCreationRequest,
  ExternalCollaborationMode,
  ExternalControlRequest,
  ExternalRuntimeRegistration,
  ProjectId,
  TaskId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

import {
  failure,
  successRoute,
  type ServiceRouteResult,
} from "./service-route-results.js";
import {
  EXTERNAL_AGENT_SESSION_CREATION_REASON_CODES,
  ExternalAgentSessionCreationError,
  ExternalThreadLifecycleError,
  type ServiceExternalRuntimeController,
} from "./service-external-runtime.js";

export interface ExternalRuntimeRouteContext {
  readonly bridge: NativeBridgeModule;
  readonly controller: ServiceExternalRuntimeController;
  startInterval(callback: () => void, intervalMs: number): NodeJS.Timeout;
  stopInterval(timer: NodeJS.Timeout): void;
  now(): string;
  requestId(request: IncomingMessage): string;
  readJsonBody(request: IncomingMessage): Promise<unknown>;
  corsHeaders(request: IncomingMessage): Record<string, string>;
}

export function isExternalRuntimeRoute(pathname: string): boolean {
  return (
    pathname === "/v1/external-runtimes" ||
    pathname.startsWith("/v1/external-runtimes/") ||
    pathname === "/v1/external-agent-sessions" ||
    pathname === "/v1/external-bindings" ||
    pathname.startsWith("/v1/external-bindings/") ||
    pathname === "/v1/external-interactions" ||
    pathname.startsWith("/v1/external-interactions/") ||
    pathname.startsWith("/v1/external-turns/") ||
    pathname.startsWith("/v1/agent-deliveries/") ||
    pathname.startsWith("/v1/agent-rounds/")
  );
}

export async function handleExternalRuntimeRequest(
  request: IncomingMessage,
  url: URL,
  context: ExternalRuntimeRouteContext,
): Promise<ServiceRouteResult> {
  const requestId = context.requestId(request);
  const method = (request.method ?? "GET").toUpperCase();
  const parts = url.pathname.split("/").filter(Boolean).map(decodeURIComponent);

  if (url.pathname === "/v1/external-agent-sessions") {
    if (method !== "POST") return methodNotAllowed(requestId);
    let creationRequest: ExternalAgentSessionCreationRequest;
    try {
      const body = requireRecord(await context.readJsonBody(request));
      creationRequest = {
        idempotencyKey: boundedRequiredString(
          body.idempotencyKey,
          256,
          "idempotencyKey",
        ),
        runtimeId: boundedRequiredString(body.runtimeId, 256, "runtimeId"),
        profileId: boundedRequiredString(body.profileId, 256, "profileId"),
        cwd: boundedRequiredString(body.cwd, 4096, "cwd"),
        ...(body.taskRef === undefined
          ? {}
          : { taskRef: optionalTaskReference(body.taskRef) }),
        ...(body.label === undefined
          ? {}
          : { label: boundedRequiredString(body.label, 256, "label") }),
        requestedAt: context.now(),
      } as ExternalAgentSessionCreationRequest;
    } catch (error) {
      return failure(400, requestId, {
        code: "invalid_input",
        reason_code: "external_agent_creation_invalid_request",
        message: error instanceof Error ? error.message : String(error),
        retryable: false,
      });
    }
    try {
      return successRoute(
        requestId,
        await context.controller.createAgentSession(creationRequest),
      );
    } catch (error) {
      return externalAgentSessionCreationFailure(requestId, error);
    }
  }

  if (url.pathname === "/v1/external-runtimes") {
    if (method === "GET") {
      return successRoute(requestId, {
        runtimes: await context.bridge.listExternalRuntimes(),
        controllers: context.controller.statuses(),
      });
    }
    if (method === "POST") {
      const body = requireRecord(await context.readJsonBody(request));
      const registration = body.registration as ExternalRuntimeRegistration;
      return successRoute(
        requestId,
        await context.bridge.registerExternalRuntime({
          registration,
          ...(numberValue(body.expectedRevision) === undefined
            ? {}
            : { expectedRevision: numberValue(body.expectedRevision) }),
        }),
      );
    }
    return methodNotAllowed(requestId);
  }

  if (url.pathname === "/v1/external-bindings") {
    if (method === "GET") {
      return successRoute(requestId, {
        bindings: await context.bridge.listExternalBindings(),
      });
    }
    if (method === "POST") {
      const body = requireRecord(await context.readJsonBody(request));
      return successRoute(
        requestId,
        await context.bridge.bindExternalAgent({
          binding: body.binding as ExternalAgentBinding,
          ...(numberValue(body.expectedRevision) === undefined
            ? {}
            : { expectedRevision: numberValue(body.expectedRevision) }),
        }),
      );
    }
    return methodNotAllowed(requestId);
  }

  if (url.pathname === "/v1/external-interactions") {
    if (method !== "GET") return methodNotAllowed(requestId);
    return successRoute(requestId, {
      interactions: await context.bridge.listPendingExternalInteractions(),
    });
  }

  if (parts[1] === "agent-rounds" && parts.length === 3) {
    if (method !== "GET") return methodNotAllowed(requestId);
    const round = await context.bridge.getAgentRound(parts[2] ?? "");
    return round === undefined
      ? notFound(requestId, "agent_round_not_found", "agent round")
      : successRoute(requestId, round);
  }

  if (parts[1] === "external-turns" && parts.length === 3) {
    if (method !== "GET") return methodNotAllowed(requestId);
    const turn = await context.bridge.getExternalTurn(parts[2] ?? "");
    return turn === undefined
      ? notFound(requestId, "external_turn_not_found", "external turn")
      : successRoute(requestId, turn);
  }

  if (parts[1] === "agent-deliveries" && parts.length === 3) {
    if (method !== "GET") return methodNotAllowed(requestId);
    const delivery = await context.bridge.getAgentMessageDelivery(
      parts[2] ?? "",
    );
    return delivery === undefined
      ? notFound(requestId, "agent_delivery_not_found", "agent delivery")
      : successRoute(requestId, delivery);
  }

  if (
    parts[1] === "external-interactions" &&
    parts.length === 4 &&
    parts[3] === "resolve"
  ) {
    if (method !== "POST") return methodNotAllowed(requestId);
    const body = requireRecord(await context.readJsonBody(request));
    return successRoute(
      requestId,
      await context.controller.resolveInteraction({
        interactionId: parts[2] ?? "",
        expectedRevision: requiredInteger(body.expectedRevision),
        idempotencyKey: requiredString(body.idempotencyKey),
        result: body.result,
      }),
    );
  }

  if (
    parts[1] === "external-bindings" &&
    parts.length === 4 &&
    parts[3] === "controls"
  ) {
    if (method !== "POST") return methodNotAllowed(requestId);
    const body = requireRecord(await context.readJsonBody(request));
    const bindingId = parts[2] ?? "";
    const binding = await context.bridge.getExternalBinding(bindingId);
    if (binding === undefined) {
      return notFound(
        requestId,
        "external_binding_not_found",
        "external binding",
      );
    }
    const controlId =
      optionalString(body.controlId) ?? `control:${crypto.randomUUID()}`;
    const control: ExternalControlRequest = {
      controlId,
      idempotencyKey:
        optionalString(body.idempotencyKey) ?? `external-control:${controlId}`,
      bindingId,
      expectedBindingRevision:
        numberValue(body.expectedBindingRevision) ?? binding.revision,
      ...(optionalString(body.expectedNativeTurnId) === undefined
        ? {}
        : { expectedNativeTurnId: optionalString(body.expectedNativeTurnId) }),
      kind: requiredString(body.kind) as ExternalControlRequest["kind"],
      payload: body.payload ?? {},
      requestedAt: context.now(),
    };
    return successRoute(
      requestId,
      await context.controller.executeControl(control),
    );
  }

  if (
    parts[1] === "external-bindings" &&
    parts.length === 4 &&
    parts[3] === "messages"
  ) {
    if (method !== "POST") return methodNotAllowed(requestId);
    const binding = await context.bridge.getExternalBinding(parts[2] ?? "");
    if (binding?.agentId == null) {
      return failure(409, requestId, {
        code: "failed_precondition",
        reason_code: "external_binding_not_routable",
        message: "external binding has no routable Crew agent identity",
        retryable: false,
      });
    }
    const body = requireRecord(await context.readJsonBody(request));
    const deliveryId =
      optionalString(body.deliveryId) ?? `operator:${randomUUID()}`;
    const idempotencyKey =
      optionalString(body.idempotencyKey) ?? `operator-message:${deliveryId}`;
    const messageId = optionalString(body.messageId) ?? `message:${deliveryId}`;
    const messageBody = requiredString(body.body);
    const collaborationMode = optionalCollaborationMode(body.collaborationMode);
    const existing = await context.bridge.getAgentMessageDelivery(deliveryId);
    if (
      existing !== undefined &&
      existing.request.idempotencyKey === idempotencyKey &&
      existing.request.messageId === messageId &&
      existing.request.toAgentId === binding.agentId &&
      existing.request.body === messageBody &&
      existing.request.collaborationMode === collaborationMode
    ) {
      return successRoute(requestId, existing);
    }
    const ttlMs = Math.min(
      Math.max(numberValue(body.ttlMs) ?? 5_000, 1),
      60_000,
    );
    const createdAt = context.now();
    return successRoute(
      requestId,
      await context.bridge.deliverAgentMessage({
        caller: { type: "system", senderAgentId: "rusty-view-operator" },
        deliveryId,
        idempotencyKey,
        messageId,
        toAgentId: binding.agentId,
        body: messageBody,
        ...(collaborationMode === undefined ? {} : { collaborationMode }),
        ...(optionalString(body.correlationId) === undefined
          ? {}
          : { correlationId: optionalString(body.correlationId) }),
        requireWake: true,
        createdAt,
        expiresAt: new Date(Date.parse(createdAt) + ttlMs).toISOString(),
      }),
    );
  }

  if (parts[1] === "external-runtimes" && parts.length >= 3) {
    const runtimeId = parts[2] ?? "";
    const registration = await context.bridge.getExternalRuntime(runtimeId);
    if (registration === undefined) {
      return notFound(
        requestId,
        "external_runtime_not_found",
        "external runtime",
      );
    }
    if (parts.length === 3) {
      return method === "GET"
        ? successRoute(requestId, {
            registration,
            controller: context.controller
              .statuses()
              .find((candidate) => candidate.runtimeId === runtimeId),
          })
        : methodNotAllowed(requestId);
    }
    switch (parts[3]) {
      case "connect":
        return method === "POST"
          ? successRoute(requestId, await context.controller.connect(runtimeId))
          : methodNotAllowed(requestId);
      case "threads":
        if (parts.length === 4 && method === "GET") {
          return successRoute(
            requestId,
            await context.controller.listThreads(runtimeId, {
              limit: numberParam(url, "limit") ?? 50,
              archived: booleanParam(url, "archived") ?? false,
              ...(stringParam(url, "cursor") === undefined
                ? {}
                : { cursor: stringParam(url, "cursor") }),
            }),
          );
        }
        if (parts[4] === "read" && method === "POST") {
          return successRoute(
            requestId,
            await context.controller.readThread(
              runtimeId,
              await context.readJsonBody(request),
            ),
          );
        }
        if (parts.length === 6 && method === "POST") {
          try {
            if (parts[5] === "archive") {
              return successRoute(
                requestId,
                await context.controller.archiveThread(
                  runtimeId,
                  parts[4] ?? "",
                ),
              );
            }
            if (parts[5] === "delete") {
              return successRoute(
                requestId,
                await context.controller.deleteThread(
                  runtimeId,
                  parts[4] ?? "",
                ),
              );
            }
            if (parts[5] === "unarchive") {
              return successRoute(
                requestId,
                await context.controller.unarchiveThread(
                  runtimeId,
                  parts[4] ?? "",
                ),
              );
            }
          } catch (error) {
            return externalThreadLifecycleFailure(requestId, error);
          }
        }
        return methodNotAllowed(requestId);
      case "events":
        if (method !== "GET") return methodNotAllowed(requestId);
        return successRoute(requestId, {
          events: await context.bridge.queryExternalRuntimeEvents({
            runtimeId,
            afterSequence: numberParam(url, "after") ?? 0,
            limit: Math.min(numberParam(url, "limit") ?? 200, 1_000),
          }),
        });
      case "stream":
        if (method !== "GET") return methodNotAllowed(requestId);
        return externalRuntimeStream(request, url, runtimeId, context);
      case "raw-details": {
        if (method !== "GET" || parts.length !== 5) {
          return methodNotAllowed(requestId);
        }
        const detail = context.controller.rawDetail(runtimeId, parts[4] ?? "");
        return detail === undefined
          ? notFound(requestId, "external_raw_detail_not_found", "raw detail")
          : successRoute(requestId, detail);
      }
    }
  }

  return failure(404, requestId, {
    code: "not_found",
    reason_code: "unknown_external_runtime_route",
    message: `unknown external runtime route ${url.pathname}`,
    retryable: false,
  });
}

function optionalCollaborationMode(
  value: unknown,
): ExternalCollaborationMode | undefined {
  if (value === undefined || value === null) return undefined;
  if (value === "plan") return value;
  throw new Error("collaborationMode must be plan when provided");
}

function externalRuntimeStream(
  request: IncomingMessage,
  url: URL,
  runtimeId: string,
  context: ExternalRuntimeRouteContext,
): ServiceRouteResult {
  const headerCursor = request.headers["last-event-id"];
  let cursor =
    typeof headerCursor === "string"
      ? Number(headerCursor)
      : (numberParam(url, "cursor") ?? 0);
  if (!Number.isSafeInteger(cursor) || cursor < 0) cursor = 0;
  const once = url.searchParams.get("once") === "true";
  return {
    kind: "raw",
    write(response) {
      response.writeHead(200, {
        "content-type": "text/event-stream; charset=utf-8",
        "cache-control": "no-cache, no-transform",
        connection: "keep-alive",
        "x-accel-buffering": "no",
        ...context.corsHeaders(request),
      });
      response.write(": connected\n\n");
      let polling = false;
      const poll = async (): Promise<void> => {
        if (polling || response.destroyed) return;
        polling = true;
        try {
          const events = await context.bridge.queryExternalRuntimeEvents({
            runtimeId,
            afterSequence: cursor,
            limit: 200,
          });
          for (const event of events) {
            cursor = event.sequenceId;
            writeExternalEvent(response, event);
          }
          if (once) response.end();
        } finally {
          polling = false;
        }
      };
      void poll();
      if (once) return;
      const timer = context.startInterval(() => void poll(), 250);
      timer.unref();
      const cleanup = (): void => {
        context.stopInterval(timer);
      };
      response.on("close", cleanup);
      response.on("error", cleanup);
    },
  };
}

function writeExternalEvent(
  response: Pick<ServerResponse, "destroyed" | "write">,
  event: Awaited<
    ReturnType<NativeBridgeModule["queryExternalRuntimeEvents"]>
  >[number],
): void {
  if (response.destroyed) return;
  response.write(`id: ${event.sequenceId}\n`);
  response.write(`event: ${event.kind}\n`);
  response.write(`data: ${JSON.stringify(event)}\n\n`);
}

function requireRecord(value: unknown): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("external runtime request body must be an object");
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown): string {
  const parsed = optionalString(value);
  if (parsed === undefined) throw new Error("required string value is missing");
  return parsed;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() !== ""
    ? value.trim()
    : undefined;
}

function boundedRequiredString(
  value: unknown,
  maxLength: number,
  field: string,
): string {
  const parsed = optionalString(value);
  if (parsed === undefined) throw new Error(`${field} is required`);
  if (parsed.length > maxLength) {
    throw new Error(`${field} exceeds ${maxLength} characters`);
  }
  return parsed;
}

function optionalTaskReference(value: unknown): DenRuntimeReference {
  const input = requireRecord(value);
  const projectId = optionalString(input.project_id);
  const taskId = optionalString(input.task_id);
  if (projectId === undefined && taskId === undefined) {
    throw new Error("taskRef requires project_id or task_id");
  }
  return {
    ...(projectId === undefined ? {} : { projectId: projectId as ProjectId }),
    ...(taskId === undefined ? {} : { taskId: taskId as TaskId }),
  };
}

function externalAgentSessionCreationFailure(
  requestId: string,
  error: unknown,
): ServiceRouteResult {
  const message = error instanceof Error ? error.message : String(error);
  const reasonCode =
    error instanceof ExternalAgentSessionCreationError
      ? error.reasonCode
      : EXTERNAL_AGENT_SESSION_CREATION_REASON_CODES.find((candidate) =>
          message.includes(candidate),
        );
  if (
    reasonCode === "external_agent_creation_idempotency_key_required" ||
    reasonCode === "external_agent_creation_profile_invalid" ||
    reasonCode === "external_agent_creation_cwd_invalid"
  ) {
    return failure(400, requestId, {
      code: "invalid_input",
      reason_code: reasonCode,
      message,
      retryable: false,
    });
  }
  if (
    reasonCode === "external_agent_creation_idempotency_conflict" ||
    reasonCode === "external_agent_creation_binding_conflict" ||
    reasonCode === "external_agent_creation_native_thread_conflict"
  ) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: reasonCode,
      message,
      retryable: false,
    });
  }
  if (reasonCode !== undefined) {
    if (reasonCode === "external_agent_creation_capacity_conflict") {
      return failure(409, requestId, {
        code: "conflict",
        reason_code: reasonCode,
        message,
        retryable: true,
      });
    }
    return failure(
      reasonCode === "external_agent_creation_native_start_failed" ? 502 : 409,
      requestId,
      {
        code: "failed_precondition",
        reason_code: reasonCode,
        message,
        retryable:
          error instanceof ExternalAgentSessionCreationError
            ? error.retryable
            : true,
      },
    );
  }
  return failure(500, requestId, {
    code: "internal_error",
    reason_code: "external_agent_creation_internal_error",
    message,
    retryable: true,
  });
}

function requiredInteger(value: unknown): number {
  const parsed = numberValue(value);
  if (parsed === undefined)
    throw new Error("required integer value is missing");
  return parsed;
}

function numberValue(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function numberParam(url: URL, name: string): number | undefined {
  const value = url.searchParams.get(name);
  return value === null ? undefined : numberValue(Number(value));
}

function stringParam(url: URL, name: string): string | undefined {
  return optionalString(url.searchParams.get(name));
}

function booleanParam(url: URL, name: string): boolean | undefined {
  const value = url.searchParams.get(name);
  if (value === null) return undefined;
  if (value === "true") return true;
  if (value === "false") return false;
  throw new Error(`${name} must be true or false`);
}

function externalThreadLifecycleFailure(
  requestId: string,
  error: unknown,
): ServiceRouteResult {
  const message = error instanceof Error ? error.message : String(error);
  const reasonCode =
    error instanceof ExternalThreadLifecycleError
      ? error.reasonCode
      : "external_thread_lifecycle_failed";
  const status =
    reasonCode === "external_thread_not_found"
      ? 404
      : reasonCode === "external_thread_active" ||
          reasonCode === "external_thread_interaction_pending"
        ? 409
        : 500;
  return failure(status, requestId, {
    code:
      status === 404
        ? "not_found"
        : status === 409
          ? "conflict"
          : "internal_error",
    reason_code: reasonCode,
    message,
    retryable: status >= 500,
  });
}

function methodNotAllowed(requestId: string): ServiceRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "external_runtime_method_not_allowed",
    message: "external runtime route does not support this method",
    retryable: false,
  });
}

function notFound(
  requestId: string,
  reasonCode: string,
  label: string,
): ServiceRouteResult {
  return failure(404, requestId, {
    code: "not_found",
    reason_code: reasonCode,
    message: `${label} was not found`,
    retryable: false,
  });
}
