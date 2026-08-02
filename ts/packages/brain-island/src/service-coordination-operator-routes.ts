import { randomUUID } from "node:crypto";
import type { IncomingMessage } from "node:http";

import type {
  AgentId,
  AgentMessageDeliveryReceipt,
  AgentRouteTarget,
  AgentRouteWrite,
  AgentRoundStartReceipt,
  ExternalBindingId,
  SessionId,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

import type { RustyCrewDeploymentRole } from "./service-config.js";
import {
  failure,
  successRoute,
  type ServiceRouteResult,
} from "./service-route-results.js";

const PRODUCTION_PREFIX = "/v1/coordination";
const DEBUG_PREFIX = "/v1/debug/coordination";

export interface CoordinationOperatorRouteContext {
  readonly bridge: NativeBridgeModule;
  readonly deploymentRole: RustyCrewDeploymentRole;
  now(): string;
  requestId(request: IncomingMessage): string;
  readJsonBody(request: IncomingMessage): Promise<unknown>;
  settleDelivery(
    receipt: AgentMessageDeliveryReceipt,
  ): Promise<AgentMessageDeliveryReceipt>;
}

export function isCoordinationOperatorRoute(pathname: string): boolean {
  return (
    pathname === PRODUCTION_PREFIX ||
    pathname.startsWith(`${PRODUCTION_PREFIX}/`) ||
    pathname === DEBUG_PREFIX ||
    pathname.startsWith(`${DEBUG_PREFIX}/`)
  );
}

export async function handleCoordinationOperatorRequest(
  request: IncomingMessage,
  url: URL,
  context: CoordinationOperatorRouteContext,
): Promise<ServiceRouteResult> {
  const requestId = context.requestId(request);
  const requestedRole = url.pathname.startsWith(DEBUG_PREFIX)
    ? "debug"
    : "production";
  if (requestedRole !== context.deploymentRole) {
    return failure(409, requestId, {
      code: "failed_precondition",
      reason_code: "coordination_deployment_role_mismatch",
      message: `${requestedRole} coordination routes are unavailable on the ${context.deploymentRole} deployment`,
      retryable: false,
    });
  }

  const prefix = requestedRole === "debug" ? DEBUG_PREFIX : PRODUCTION_PREFIX;
  const suffix = url.pathname.slice(prefix.length);
  const parts = suffix.split("/").filter(Boolean).map(decodeURIComponent);
  const method = (request.method ?? "GET").toUpperCase();

  if (parts.length === 1 && parts[0] === "agents") {
    if (method !== "GET") return methodNotAllowed(requestId);
    return successRoute(requestId, {
      deploymentRole: context.deploymentRole,
      agents: await context.bridge.listAgentDirectory(),
    });
  }

  if (parts.length === 1 && parts[0] === "routes") {
    if (method === "GET") {
      return successRoute(requestId, {
        deploymentRole: context.deploymentRole,
        routes: await context.bridge.listAgentRouteResolutions(),
      });
    }
    if (method !== "POST") return methodNotAllowed(requestId);
    try {
      const body = requireRecord(await context.readJsonBody(request));
      const route = await context.bridge.putAgentRoute(
        agentRouteWrite(body, requiredString(body.routeKey), context.now()),
      );
      return successRoute(requestId, {
        deploymentRole: context.deploymentRole,
        route,
        resolution: await context.bridge.getAgentRouteResolution(
          route.routeKey,
        ),
      });
    } catch (error) {
      return routeWriteFailure(requestId, error);
    }
  }

  if (parts.length === 2 && parts[0] === "routes" && parts[1] === "resolve") {
    if (method !== "POST") return methodNotAllowed(requestId);
    try {
      const body = requireRecord(await context.readJsonBody(request));
      return successRoute(requestId, {
        deploymentRole: context.deploymentRole,
        resolution: await context.bridge.resolveAgentAddress(
          requiredString(body.address),
        ),
      });
    } catch (error) {
      return routeWriteFailure(requestId, error);
    }
  }

  if (parts.length === 2 && parts[0] === "routes") {
    const routeKey = parts[1] ?? "";
    if (method === "GET") {
      const resolution = await context.bridge.getAgentRouteResolution(routeKey);
      return resolution === undefined
        ? notFound(requestId, "agent_route_not_found")
        : successRoute(requestId, {
            deploymentRole: context.deploymentRole,
            resolution,
          });
    }
    if (method === "PATCH") {
      try {
        const body = requireRecord(await context.readJsonBody(request));
        const route = await context.bridge.putAgentRoute(
          agentRouteWrite(body, routeKey, context.now()),
        );
        return successRoute(requestId, {
          deploymentRole: context.deploymentRole,
          route,
          resolution: await context.bridge.getAgentRouteResolution(routeKey),
        });
      } catch (error) {
        return routeWriteFailure(requestId, error);
      }
    }
    if (method === "DELETE") {
      try {
        const expectedRevision = requiredPositiveInteger(
          url.searchParams.get("expectedRevision"),
          "expectedRevision",
        );
        return successRoute(requestId, {
          deploymentRole: context.deploymentRole,
          route: await context.bridge.deleteAgentRoute({
            routeKey,
            expectedRevision,
          }),
        });
      } catch (error) {
        return routeWriteFailure(requestId, error);
      }
    }
    return methodNotAllowed(requestId);
  }

  if (parts.length === 3 && parts[0] === "routes" && parts[2] === "test") {
    if (method !== "POST") return methodNotAllowed(requestId);
    try {
      const body = requireRecord(await context.readJsonBody(request));
      const ids = commandIds(body, "operator-route-test");
      const createdAt = context.now();
      const initialReceipt = await context.bridge.deliverAgentMessage({
        caller: {
          type: "system",
          senderAgentId: operatorAgentId(context.deploymentRole),
        },
        deliveryId: ids.deliveryId,
        idempotencyKey: ids.idempotencyKey,
        messageId: ids.messageId,
        toAddress: `@${parts[1] ?? ""}`,
        inputKind: "routed_agent_message",
        body: requiredString(body.body),
        ...(optionalString(body.correlationId) === undefined
          ? {}
          : { correlationId: optionalString(body.correlationId) }),
        requireWake: optionalBoolean(body.requireWake) ?? true,
        createdAt,
        expiresAt: expiresAt(createdAt, body.ttlMs),
      });
      const receipt = await context.settleDelivery(initialReceipt);
      return operatorDeliveryResult(requestId, context.deploymentRole, receipt);
    } catch (error) {
      return invalidInput(requestId, error);
    }
  }

  if (parts.length === 1 && parts[0] === "messages") {
    if (method === "GET") {
      const toAgentId = url.searchParams.get("toAgentId")?.trim();
      const toSessionId = url.searchParams.get("toSessionId")?.trim();
      const fromAgentId = url.searchParams.get("fromAgentId")?.trim();
      const fromSessionId = url.searchParams.get("fromSessionId")?.trim();
      const correlationId = url.searchParams.get("correlationId")?.trim();
      const messageId = url.searchParams.get("messageId")?.trim();
      const limitText = url.searchParams.get("limit")?.trim();
      const limit = limitText === undefined ? 100 : Number(limitText);
      if (!Number.isInteger(limit) || limit < 1 || limit > 500) {
        return invalidInput(
          requestId,
          new Error("limit must be an integer from 1 to 500"),
        );
      }
      return successRoute(requestId, {
        deploymentRole: context.deploymentRole,
        items: await context.bridge.listAgentMessageTraffic({
          ...(toAgentId === undefined || toAgentId === ""
            ? {}
            : { toAgentId: toAgentId as AgentId }),
          ...(toSessionId === undefined || toSessionId === ""
            ? {}
            : { toSessionId: toSessionId as SessionId }),
          ...(fromAgentId === undefined || fromAgentId === ""
            ? {}
            : { fromAgentId: fromAgentId as AgentId }),
          ...(fromSessionId === undefined || fromSessionId === ""
            ? {}
            : { fromSessionId: fromSessionId as SessionId }),
          ...(correlationId === undefined || correlationId === ""
            ? {}
            : { correlationId }),
          ...(messageId === undefined || messageId === "" ? {} : { messageId }),
          limit,
        }),
      });
    }
    if (method !== "POST") return methodNotAllowed(requestId);
    try {
      const body = requireRecord(await context.readJsonBody(request));
      const ids = commandIds(body, "operator-message");
      const createdAt = context.now();
      const initialReceipt = await context.bridge.deliverAgentMessage({
        caller: {
          type: "system",
          senderAgentId: operatorAgentId(context.deploymentRole),
        },
        deliveryId: ids.deliveryId,
        idempotencyKey: ids.idempotencyKey,
        messageId: ids.messageId,
        toAddress: requiredString(body.toAddress),
        inputKind: "routed_agent_message",
        body: requiredString(body.body),
        ...(optionalString(body.correlationId) === undefined
          ? {}
          : { correlationId: optionalString(body.correlationId) }),
        requireWake: true,
        createdAt,
        expiresAt: expiresAt(createdAt, body.ttlMs),
      });
      const receipt = await context.settleDelivery(initialReceipt);
      return operatorDeliveryResult(requestId, context.deploymentRole, receipt);
    } catch (error) {
      return invalidInput(requestId, error);
    }
  }

  if (parts.length === 1 && parts[0] === "rounds") {
    if (method !== "POST") return methodNotAllowed(requestId);
    try {
      const body = requireRecord(await context.readJsonBody(request));
      const ids = commandIds(body, "operator-round");
      const createdAt = context.now();
      const roundId = optionalString(body.roundId) ?? `round:${randomUUID()}`;
      const correlationId =
        optionalString(body.correlationId) ?? `correlation:${roundId}`;
      const senderAgentId = operatorAgentId(context.deploymentRole);
      const started = await context.bridge.beginAgentRound({
        caller: {
          type: "system",
          senderAgentId,
        },
        roundId,
        idempotencyKey: ids.idempotencyKey,
        messageId: ids.messageId,
        toAddress: requiredString(body.toAddress),
        body: operatorRoundBody({
          senderAgentId,
          correlationId,
          request: requiredString(body.body),
        }),
        correlationId,
        createdAt,
        expiresAt: expiresAt(createdAt, body.ttlMs),
      });
      return operatorRoundResult(requestId, context.deploymentRole, started);
    } catch (error) {
      return invalidInput(requestId, error);
    }
  }

  if (parts.length === 2 && parts[0] === "deliveries") {
    if (method !== "GET") return methodNotAllowed(requestId);
    const receipt = await context.bridge.getAgentMessageDelivery(
      parts[1] ?? "",
    );
    return receipt === undefined
      ? notFound(requestId, "agent_delivery_not_found")
      : operatorDeliveryResult(requestId, context.deploymentRole, receipt);
  }

  if (parts.length === 2 && parts[0] === "rounds") {
    if (method !== "GET") return methodNotAllowed(requestId);
    const round = await context.bridge.getAgentRound(parts[1] ?? "");
    return round === undefined
      ? notFound(requestId, "agent_round_not_found")
      : successRoute(requestId, {
          deploymentRole: context.deploymentRole,
          targetAgentId: round.recipientAgentId,
          deliveryId: `round-delivery:${round.roundId}`,
          roundId: round.roundId,
          status: round.status,
          terminalReason: round.terminalReasonCode ?? null,
          round,
        });
  }

  return notFound(requestId, "coordination_operator_route_not_found");
}

function operatorRoundBody(input: {
  senderAgentId: AgentId;
  correlationId: string;
  request: string;
}): string {
  return [
    "Rusty Crew correlated operator round:",
    `After completing the request, call your Rusty Crew send_agent_message tool exactly once with recipient ${input.senderAgentId}, correlationId ${input.correlationId}, and your reply as the body. Do not omit the correlation ID.`,
    "",
    "Request:",
    input.request,
  ].join("\n");
}

function operatorDeliveryResult(
  requestId: string,
  deploymentRole: RustyCrewDeploymentRole,
  receipt: AgentMessageDeliveryReceipt,
): ServiceRouteResult {
  return successRoute(requestId, {
    deploymentRole,
    targetAgentId: receipt.request.toAgentId,
    deliveryId: receipt.request.deliveryId,
    roundId: receipt.resolvedRoundId ?? null,
    status: receipt.status,
    terminalReason: receipt.reasonCode ?? null,
    delivery: receipt,
  });
}

function operatorRoundResult(
  requestId: string,
  deploymentRole: RustyCrewDeploymentRole,
  started: AgentRoundStartReceipt,
): ServiceRouteResult {
  return successRoute(requestId, {
    deploymentRole,
    targetAgentId: started.round.recipientAgentId,
    deliveryId: started.delivery.request.deliveryId,
    roundId: started.round.roundId,
    status: started.round.status,
    terminalReason: started.round.terminalReasonCode ?? null,
    delivery: started.delivery,
    round: started.round,
  });
}

function commandIds(
  body: Record<string, unknown>,
  prefix: string,
): { deliveryId: string; idempotencyKey: string; messageId: string } {
  const deliveryId =
    optionalString(body.deliveryId) ?? `${prefix}:${randomUUID()}`;
  return {
    deliveryId,
    idempotencyKey:
      optionalString(body.idempotencyKey) ?? `${prefix}:${deliveryId}`,
    messageId: optionalString(body.messageId) ?? `message:${deliveryId}`,
  };
}

function expiresAt(createdAt: string, rawTtlMs: unknown): string {
  const ttlMs = Math.min(
    Math.max(optionalInteger(rawTtlMs) ?? 30_000, 1),
    300_000,
  );
  return new Date(Date.parse(createdAt) + ttlMs).toISOString();
}

function agentRouteWrite(
  body: Record<string, unknown>,
  routeKey: string,
  updatedAt: string,
): AgentRouteWrite {
  const expectedRevision = optionalPositiveInteger(
    body.expectedRevision,
    "expectedRevision",
  );
  const requiredRuntimeKind = optionalEnum(
    body.requiredRuntimeKind,
    ["direct_brain", "codex_app_server"] as const,
    "requiredRuntimeKind",
  );
  const requiredDeliveryPolicy = optionalEnum(
    body.requiredDeliveryPolicy,
    ["immediate_steer", "serial_next_turn"] as const,
    "requiredDeliveryPolicy",
  );
  return {
    routeKey,
    label: requiredString(body.label),
    ...(body.description === undefined || body.description === null
      ? {}
      : { description: requiredString(body.description) }),
    enabled: optionalBoolean(body.enabled) ?? true,
    target: agentRouteTarget(body.target),
    ...(requiredRuntimeKind === undefined ? {} : { requiredRuntimeKind }),
    ...(requiredDeliveryPolicy === undefined ? {} : { requiredDeliveryPolicy }),
    ...(expectedRevision === undefined ? {} : { expectedRevision }),
    updatedAt,
  };
}

function agentRouteTarget(value: unknown): AgentRouteTarget {
  const target = requireRecord(value);
  const type = requiredString(target.type);
  if (type === "direct_brain") {
    return {
      type,
      agentId: requiredString(target.agentId) as AgentId,
      sessionId: requiredString(target.sessionId) as SessionId,
    };
  }
  if (type === "managed_external") {
    return {
      type,
      agentId: requiredString(target.agentId) as AgentId,
      bindingId: requiredString(target.bindingId) as ExternalBindingId,
      bindingRevision: requiredPositiveInteger(
        target.bindingRevision,
        "target.bindingRevision",
      ),
    };
  }
  throw new Error("target.type must be direct_brain or managed_external");
}

function optionalEnum<const T extends readonly string[]>(
  value: unknown,
  allowed: T,
  field: string,
): T[number] | undefined {
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string" || !allowed.includes(value)) {
    throw new Error(`${field} must be one of ${allowed.join(", ")}`);
  }
  return value as T[number];
}

function optionalBoolean(value: unknown): boolean | undefined {
  if (value === undefined) return undefined;
  if (typeof value !== "boolean") throw new Error("expected a boolean");
  return value;
}

function optionalPositiveInteger(
  value: unknown,
  field: string,
): number | undefined {
  if (value === undefined || value === null) return undefined;
  return requiredPositiveInteger(value, field);
}

function requiredPositiveInteger(value: unknown, field: string): number {
  const parsed =
    typeof value === "string" && value.trim() !== "" ? Number(value) : value;
  if (
    typeof parsed !== "number" ||
    !Number.isSafeInteger(parsed) ||
    parsed < 1
  ) {
    throw new Error(`${field} must be a positive integer`);
  }
  return parsed;
}

function operatorAgentId(role: RustyCrewDeploymentRole): AgentId {
  return `rusty-crew-${role}-operator` as AgentId;
}

function requireRecord(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("request body must be a JSON object");
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error("required string field is missing or empty");
  }
  return value;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim().length > 0
    ? value
    : undefined;
}

function optionalInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value)
    ? value
    : undefined;
}

function invalidInput(requestId: string, error: unknown): ServiceRouteResult {
  return failure(400, requestId, {
    code: "invalid_input",
    reason_code: "coordination_operator_invalid_request",
    message: error instanceof Error ? error.message : String(error),
    retryable: false,
  });
}

function routeWriteFailure(
  requestId: string,
  error: unknown,
): ServiceRouteResult {
  const message = error instanceof Error ? error.message : String(error);
  if (
    message.includes("agent_route_revision_mismatch") ||
    message.includes("agent_route_already_exists")
  ) {
    return failure(409, requestId, {
      code: "conflict",
      reason_code: message.includes("revision_mismatch")
        ? "agent_route_revision_mismatch"
        : "agent_route_already_exists",
      message,
      retryable: false,
    });
  }
  if (message.includes("agent_route_not_found")) {
    return notFound(requestId, "agent_route_not_found");
  }
  return invalidInput(requestId, error);
}

function methodNotAllowed(requestId: string): ServiceRouteResult {
  return failure(405, requestId, {
    code: "method_not_allowed",
    reason_code: "coordination_operator_method_not_allowed",
    message: "method not allowed",
    retryable: false,
  });
}

function notFound(requestId: string, reasonCode: string): ServiceRouteResult {
  return failure(404, requestId, {
    code: "not_found",
    reason_code: reasonCode,
    message: "coordination operator record or route was not found",
    retryable: false,
  });
}
