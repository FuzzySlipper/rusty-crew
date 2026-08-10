import type {
  NativeBridgeModule,
  NativeInstallDiplomatBindingRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import type { AdminRouteResult } from "./admin-diagnostics-api.js";
import type { RustyCrewTelegramConfig } from "./service-config.js";
import type { TelegramChannelConnectorPort } from "./service-adapter-ports.js";
import { failure, successRoute } from "./service-route-results.js";

export interface TelegramDiplomatAdminRequest {
  method: string;
  url: string;
  body?: unknown;
  requestId: string;
}

export interface TelegramDiplomatAdminContext {
  bridge: NativeBridgeModule;
  config: RustyCrewTelegramConfig;
  connector(): TelegramChannelConnectorPort | undefined;
  restartConnector(): Promise<void>;
  now(): string;
}

export async function handleTelegramDiplomatAdminRequest(
  request: TelegramDiplomatAdminRequest,
  context: TelegramDiplomatAdminContext,
): Promise<AdminRouteResult> {
  try {
    return await handleTelegramDiplomatAdminRequestInner(request, context);
  } catch (error) {
    return failure(409, request.requestId, {
      code: "conflict",
      reason_code: telegramDiplomatErrorReason(error),
      message: error instanceof Error ? error.message : String(error),
      retryable: false,
    });
  }
}

async function handleTelegramDiplomatAdminRequestInner(
  request: TelegramDiplomatAdminRequest,
  context: TelegramDiplomatAdminContext,
): Promise<AdminRouteResult> {
  const url = new URL(request.url);
  const method = request.method.toUpperCase();
  const segments = url.pathname
    .replace(/^\/v1\/admin\/telegram-diplomat\/?/, "")
    .split("/")
    .filter(Boolean)
    .map(decodeURIComponent);

  if (method === "GET" && segments.length === 0) {
    return successRoute(
      request.requestId,
      await telegramDiplomatReadback(context),
    );
  }
  if (method === "POST" && segments[0] === "credential") {
    const body = recordBody(request.body);
    const token = requiredString(body.token, "token");
    const existing = await context.bridge.getServiceCredential(
      context.config.credentialId,
    );
    await context.bridge.upsertServiceCredential({
      credentialId: context.config.credentialId,
      displayName:
        optionalString(body.displayName) ?? "Telegram install diplomat bot",
      providerKind: "telegram",
      credentialKind: "api_key",
      secret: token,
      clearSecret: false,
      expectedRevision:
        optionalNumber(body.expectedRevision) ?? existing?.revision,
      now: context.now(),
    });
    await restartConnectorOrDegrade(context);
    return successRoute(request.requestId, {
      tokenUpdated: true,
      ...(await telegramDiplomatReadback(context)),
    });
  }
  if (method === "POST" && segments[0] === "reload") {
    await restartConnectorOrDegrade(context);
    return successRoute(
      request.requestId,
      await telegramDiplomatReadback(context),
    );
  }
  if (
    method === "POST" &&
    segments[0] === "bindings" &&
    segments.length === 1
  ) {
    const body = recordBody(request.body);
    const identity = context.connector()?.diagnostics().botIdentity;
    if (identity === undefined) {
      return failure(409, request.requestId, {
        code: "conflict",
        reason_code: "telegram_diplomat_connector_not_identified",
        message:
          "Telegram connector must identify its bot before a binding can become active.",
        retryable: true,
      });
    }
    const binding = await context.bridge.putInstallDiplomatBinding({
      bindingId: requiredString(body.bindingId, "bindingId"),
      expectedRevision: optionalNumber(body.expectedRevision),
      installationId: requiredString(body.installationId, "installationId"),
      installationLabel: requiredString(
        body.installationLabel,
        "installationLabel",
      ),
      adapterId: context.config.adapterId as never,
      botUserId: identity.userId,
      botUsername:
        identity.username ?? identity.displayLabel ?? identity.userId,
      agentId: requiredString(body.agentId, "agentId") as never,
      instanceId: optionalString(body.instanceId) as never,
      sessionId: requiredString(body.sessionId, "sessionId") as never,
      externalChatId: requiredString(body.externalChatId, "externalChatId"),
      externalThreadId: optionalString(body.externalThreadId),
      participationMode:
        body.participationMode === "topic_human_messages"
          ? "topic_human_messages"
          : "mention_or_reply",
      updatedAt: context.now(),
    });
    await restartConnectorOrDegrade(context);
    return successRoute(request.requestId, { binding });
  }
  const bindingId = segments[1];
  if (method === "GET" && segments[0] === "bindings" && bindingId) {
    const binding = await context.bridge.getInstallDiplomatBinding(bindingId);
    return binding === undefined
      ? failure(404, request.requestId, {
          code: "not_found",
          reason_code: "telegram_diplomat_binding_not_found",
          message: `Telegram diplomat binding ${bindingId} was not found.`,
          retryable: false,
        })
      : successRoute(request.requestId, { binding });
  }
  if (method === "POST" && segments[0] === "bindings" && bindingId) {
    const body = recordBody(request.body);
    const existing = await requiredBinding(context, bindingId);
    const expectedRevision =
      optionalNumber(body.expectedRevision) ?? existing.revision;
    if (segments[2] === "relabel") {
      const binding = await context.bridge.putInstallDiplomatBinding({
        bindingId,
        expectedRevision,
        installationId: existing.installationId,
        installationLabel: requiredString(
          body.installationLabel,
          "installationLabel",
        ),
        adapterId: existing.adapterId as never,
        botUserId: existing.botUserId,
        botUsername: existing.botUsername,
        agentId: existing.agentId as never,
        instanceId: (existing.instanceId ?? undefined) as never,
        sessionId: existing.sessionId as never,
        externalChatId: existing.externalChatId,
        externalThreadId: existing.externalThreadId ?? undefined,
        participationMode: existing.participationMode,
        updatedAt: context.now(),
      });
      return successRoute(request.requestId, { binding });
    }
    if (segments[2] === "move") {
      const binding = await context.bridge.rebindInstallDiplomat({
        bindingId,
        expectedRevision,
        agentId: requiredString(body.agentId, "agentId") as never,
        instanceId: optionalString(body.instanceId) as never,
        sessionId: requiredString(body.sessionId, "sessionId") as never,
        updatedAt: context.now(),
      });
      await restartConnectorOrDegrade(context);
      return successRoute(request.requestId, { binding });
    }
    const status =
      segments[2] === "pause"
        ? "paused"
        : segments[2] === "resume"
          ? "active"
          : segments[2] === "remove"
            ? "removed"
            : undefined;
    if (status !== undefined) {
      const binding = await context.bridge.setInstallDiplomatBindingStatus({
        bindingId,
        expectedRevision,
        status,
        degradedReason: undefined,
        updatedAt: context.now(),
      });
      await restartConnectorOrDegrade(context);
      return successRoute(request.requestId, { binding });
    }
  }
  return failure(405, request.requestId, {
    code: "method_not_allowed",
    reason_code: "telegram_diplomat_method_not_allowed",
    message: "Unsupported Telegram diplomat admin operation.",
    retryable: false,
  });
}

async function telegramDiplomatReadback(context: TelegramDiplomatAdminContext) {
  const [credential, bindings] = await Promise.all([
    context.bridge.getServiceCredential(context.config.credentialId),
    context.bridge.listInstallDiplomatBindings({
      adapterId: context.config.adapterId as never,
    }),
  ]);
  const diagnostics = context.connector()?.diagnostics();
  return {
    state: telegramDiplomatState(
      context.config,
      credential,
      bindings,
      diagnostics,
    ),
    enabled: context.config.enabled,
    adapterId: context.config.adapterId,
    credentialId: context.config.credentialId,
    credential,
    botIdentity: diagnostics?.botIdentity,
    candidates: diagnostics?.candidates ?? [],
    bindings,
    connector: diagnostics,
  };
}

export function telegramDiplomatState(
  config: RustyCrewTelegramConfig,
  credential: NativeServiceCredentialRecord | undefined,
  bindings: readonly NativeInstallDiplomatBindingRecord[],
  diagnostics:
    | ReturnType<TelegramChannelConnectorPort["diagnostics"]>
    | undefined,
):
  | "disabled"
  | "unconfigured"
  | "disconnected"
  | "unbound"
  | "ambiguous"
  | "rate_limited"
  | "healthy" {
  if (!config.enabled) return "disabled";
  if (!credential?.credential.hasSecret && !config.botToken)
    return "unconfigured";
  if (diagnostics === undefined || diagnostics.lastError) {
    return diagnostics?.lastError?.includes("429")
      ? "rate_limited"
      : "disconnected";
  }
  if (
    diagnostics.botIdentity !== undefined &&
    bindings.some(
      (binding) =>
        binding.status === "active" &&
        binding.botUserId !== diagnostics.botIdentity?.userId,
    )
  ) {
    return "disconnected";
  }
  if (diagnostics.inbound?.rateLimited > 0) return "rate_limited";
  if (diagnostics.inbound?.ambiguous > 0) return "ambiguous";
  if (!bindings.some((binding) => binding.status === "active"))
    return "unbound";
  return "healthy";
}

async function requiredBinding(
  context: TelegramDiplomatAdminContext,
  bindingId: string,
): Promise<NativeInstallDiplomatBindingRecord> {
  const binding = await context.bridge.getInstallDiplomatBinding(bindingId);
  if (binding === undefined)
    throw new Error(`Telegram diplomat binding ${bindingId} was not found`);
  return binding;
}

async function restartConnectorOrDegrade(
  context: TelegramDiplomatAdminContext,
): Promise<void> {
  try {
    await context.restartConnector();
    const diagnostics = context.connector()?.diagnostics();
    if (
      context.config.enabled &&
      (diagnostics === undefined ||
        diagnostics.lastError !== undefined ||
        diagnostics.botIdentity === undefined)
    ) {
      throw new Error(
        diagnostics?.lastError ?? "Telegram connector did not identify its bot",
      );
    }
  } catch (error) {
    const active = await context.bridge.listInstallDiplomatBindings({
      adapterId: context.config.adapterId as never,
      status: "active",
    });
    await Promise.allSettled(
      active.map((binding) =>
        context.bridge.setInstallDiplomatBindingStatus({
          bindingId: binding.bindingId,
          expectedRevision: binding.revision,
          status: "needs_rebind",
          degradedReason: "telegram_connector_reload_failed",
          updatedAt: context.now(),
        }),
      ),
    );
    throw error;
  }
}

function recordBody(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("request body must be a JSON object");
  }
  return value as Record<string, unknown>;
}

function requiredString(value: unknown, field: string): string {
  const result = optionalString(value);
  if (result === undefined) throw new Error(`${field} is required`);
  return result;
}

function optionalString(value: unknown): string | undefined {
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function optionalNumber(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function telegramDiplomatErrorReason(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("revision"))
    return "telegram_diplomat_revision_conflict";
  if (message.includes("not found"))
    return "telegram_diplomat_binding_not_found";
  return "telegram_diplomat_admin_failed";
}
