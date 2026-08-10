import assert from "node:assert/strict";
import test from "node:test";
import type {
  NativeBridgeModule,
  NativeInstallDiplomatBindingRecord,
  NativeServiceCredentialRecord,
} from "@rusty-crew/native-bridge";
import {
  handleTelegramDiplomatAdminRequest,
  telegramDiplomatState,
  type TelegramDiplomatAdminContext,
} from "../src/telegram-diplomat-admin-routes.js";

const now = "2026-08-10T12:00:00Z";

function binding(
  overrides: Partial<NativeInstallDiplomatBindingRecord> = {},
): NativeInstallDiplomatBindingRecord {
  return {
    schemaVersion: "telegram_install_diplomat.v1",
    bindingId: "binding-1",
    revision: 1,
    installationId: "install-1",
    installationLabel: "Workshop",
    adapterId: "telegram-main" as never,
    botUserId: "42",
    botUsername: "diplomat_bot",
    agentId: "agent-1" as never,
    sessionId: "session-old" as never,
    externalChatId: "-1001",
    participationMode: "mention_or_reply",
    status: "active",
    createdAt: now,
    updatedAt: now,
    ...overrides,
  };
}

function credential(hasSecret = true): NativeServiceCredentialRecord {
  return {
    credentialId: "telegram-main",
    displayName: "Telegram",
    providerKind: "telegram",
    credentialKind: "api_key",
    credential: { hasSecret },
    linkedProviderAliases: [],
    revision: 1,
    createdAt: now,
    updatedAt: now,
  };
}

function context(
  input: {
    identified?: boolean;
    records?: NativeInstallDiplomatBindingRecord[];
    restartError?: Error;
    statusUpdates?: Array<Record<string, unknown>>;
    rebind?: (value: unknown) => Promise<NativeInstallDiplomatBindingRecord>;
  } = {},
): TelegramDiplomatAdminContext {
  const records = input.records ?? [];
  const bridge = {
    getServiceCredential: async () => credential(),
    listInstallDiplomatBindings: async () => records,
    getInstallDiplomatBinding: async (id: string) =>
      records.find((item) => item.bindingId === id),
    putInstallDiplomatBinding: async (write: Record<string, unknown>) => {
      const created = binding({
        bindingId: String(write.bindingId),
        sessionId: write.sessionId as never,
      });
      records.push(created);
      return created;
    },
    rebindInstallDiplomat:
      input.rebind ??
      (async (request: Record<string, unknown>) =>
        binding({
          revision: 2,
          sessionId: request.sessionId as never,
        })),
    setInstallDiplomatBindingStatus: async (
      update: Record<string, unknown>,
    ) => {
      input.statusUpdates?.push(update);
      return binding({
        status: update.status as NativeInstallDiplomatBindingRecord["status"],
        revision: Number(update.expectedRevision) + 1,
      });
    },
  } as unknown as NativeBridgeModule;
  return {
    bridge,
    config: {
      enabled: true,
      adapterId: "telegram-main",
      credentialId: "telegram-main",
      pollIntervalMs: 2_000,
      pollTimeoutSeconds: 20,
      updateLimit: 50,
      messageTtlMs: 300_000,
    },
    connector: () => ({
      start: async () => undefined,
      stop: () => undefined,
      pollOnce: async () => undefined,
      sendOutbound: async () => undefined,
      diagnostics: () => ({
        bindingCount: records.length,
        ...(input.identified === false
          ? {}
          : { botIdentity: { userId: "42", username: "diplomat_bot" } }),
        candidates: [],
      }),
    }),
    restartConnector: async () => {
      if (input.restartError !== undefined) throw input.restartError;
    },
    now: () => now,
  };
}

test("diagnostics distinguish all required operator states", () => {
  const config = context().config;
  assert.equal(
    telegramDiplomatState(
      { ...config, enabled: false },
      credential(),
      [],
      undefined,
    ),
    "disabled",
  );
  assert.equal(
    telegramDiplomatState(config, credential(false), [], undefined),
    "unconfigured",
  );
  assert.equal(
    telegramDiplomatState(config, credential(), [], undefined),
    "disconnected",
  );
  assert.equal(
    telegramDiplomatState(config, credential(), [], { bindingCount: 0 }),
    "unbound",
  );
  assert.equal(
    telegramDiplomatState(config, credential(), [binding()], {
      bindingCount: 1,
      inbound: { ambiguous: 1 },
    } as never),
    "ambiguous",
  );
  assert.equal(
    telegramDiplomatState(config, credential(), [binding()], {
      bindingCount: 1,
      lastError: "Telegram 429",
    }),
    "rate_limited",
  );
  assert.equal(
    telegramDiplomatState(config, credential(), [binding()], {
      bindingCount: 1,
    }),
    "healthy",
  );
});

test("binding creation fails before bot identity and cannot appear active", async () => {
  const result = await handleTelegramDiplomatAdminRequest(
    {
      method: "POST",
      url: "http://localhost/v1/admin/telegram-diplomat/bindings",
      body: {
        bindingId: "binding-1",
        installationId: "install-1",
        installationLabel: "Workshop",
        agentId: "agent-1",
        sessionId: "session-1",
        externalChatId: "-1001",
      },
      requestId: "request-1",
    },
    context({ identified: false }),
  );
  assert.equal(result.status, 409);
  assert.equal(result.body.ok, false);
});

test("move changes only the exact target session and reports revision conflicts", async () => {
  let request: Record<string, unknown> | undefined;
  const existing = binding();
  const moved = await handleTelegramDiplomatAdminRequest(
    {
      method: "POST",
      url: "http://localhost/v1/admin/telegram-diplomat/bindings/binding-1/move",
      body: {
        sessionId: "session-new",
        agentId: "agent-2",
        expectedRevision: 1,
      },
      requestId: "request-2",
    },
    context({
      records: [existing],
      rebind: async (value) => {
        request = value as Record<string, unknown>;
        return binding({
          revision: 2,
          sessionId: "session-new" as never,
          agentId: "agent-2" as never,
        });
      },
    }),
  );
  assert.equal(moved.status, 200);
  assert.equal(request?.sessionId, "session-new");
  assert.equal(existing.sessionId, "session-old");

  const conflict = await handleTelegramDiplomatAdminRequest(
    {
      method: "POST",
      url: "http://localhost/v1/admin/telegram-diplomat/bindings/binding-1/move",
      body: {
        sessionId: "session-new",
        agentId: "agent-2",
        expectedRevision: 0,
      },
      requestId: "request-3",
    },
    context({
      records: [existing],
      rebind: async () => {
        throw new Error("install diplomat binding revision conflict");
      },
    }),
  );
  assert.equal(conflict.status, 409);
  assert.equal(conflict.body.ok, false);
  if (!conflict.body.ok)
    assert.equal(
      conflict.body.error.reason_code,
      "telegram_diplomat_revision_conflict",
    );
});

test("connector reload failure degrades a newly written active binding", async () => {
  const updates: Array<Record<string, unknown>> = [];
  const result = await handleTelegramDiplomatAdminRequest(
    {
      method: "POST",
      url: "http://localhost/v1/admin/telegram-diplomat/bindings",
      body: {
        bindingId: "binding-new",
        installationId: "install-1",
        installationLabel: "Workshop",
        agentId: "agent-1",
        sessionId: "session-1",
        externalChatId: "-1001",
      },
      requestId: "request-4",
    },
    context({
      restartError: new Error("connector reload failed"),
      statusUpdates: updates,
    }),
  );
  assert.equal(result.status, 409);
  assert.equal(updates.length, 1);
  assert.equal(updates[0]?.status, "needs_rebind");
  assert.equal(updates[0]?.degradedReason, "telegram_connector_reload_failed");
});
