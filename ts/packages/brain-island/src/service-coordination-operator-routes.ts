import { randomUUID } from "node:crypto";
import type { IncomingMessage } from "node:http";

import type {
  AgentId,
  AgentMessageDeliveryReceipt,
  AgentRoundStartReceipt,
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

  if (parts.length === 1 && parts[0] === "messages") {
    if (method === "GET") {
      const toAgentId = url.searchParams.get("toAgentId")?.trim();
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
        items: await context.bridge.listAgentMessageInbox({
          ...(toAgentId === undefined || toAgentId === ""
            ? {}
            : { toAgentId: toAgentId as AgentId }),
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
        toAgentId: requiredString(body.toAgentId) as AgentId,
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
        toAgentId: requiredString(body.toAgentId) as AgentId,
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
