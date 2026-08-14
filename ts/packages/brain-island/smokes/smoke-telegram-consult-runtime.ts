import assert from "node:assert/strict";
import type {
  NormalizedChannelOutboundMessage,
  TelegramOperatorConsultRecord,
  TelegramOperatorConsultSettlement,
} from "@rusty-crew/contracts";
import {
  createServiceTelegramConsultRuntime,
  recoverPendingTelegramOperatorConsults,
} from "../src/service-app.js";

const baseRecord: TelegramOperatorConsultRecord = {
  schemaVersion: "telegram_operator_consult.v1",
  consultId: "telegram-consult-1",
  idempotencyKey: "session-1:wake-1:call-1",
  revision: 1,
  bindingId: "diplomat-binding",
  adapterId: "telegram-alpha",
  agentId: "diplomat",
  sessionId: "session-1",
  profileId: "ambassador-profile",
  wakeId: "wake-1",
  toolCallId: "call-1",
  originatingWakeKind: "operator",
  category: "network_trouble",
  body: "Network state is ambiguous. Should I inspect the router?",
  externalChatId: "-100500",
  externalThreadId: "42",
  status: "pending",
  deliveryAttempts: 0,
  externalMessageIds: [],
  requestedAt: "2026-08-13T20:00:00Z",
  updatedAt: "2026-08-13T20:00:00Z",
};

function harness(options: {
  connector?: {
    sendOutbound(message: NormalizedChannelOutboundMessage): Promise<unknown>;
  };
  listed?: TelegramOperatorConsultRecord[];
}) {
  let record = { ...baseRecord };
  const settlements: TelegramOperatorConsultSettlement[] = [];
  const state = {
    bridge: {
      async requestTelegramOperatorConsult() {
        return record;
      },
      async prepareTelegramOperatorConsultDelivery() {
        return record;
      },
      async settleTelegramOperatorConsult(
        settlement: TelegramOperatorConsultSettlement,
      ) {
        settlements.push(settlement);
        record = {
          ...record,
          revision: record.revision + 1,
          status: settlement.status,
          deliveryAttempts: settlement.deliveryAttempts,
          externalMessageIds: settlement.externalMessageIds,
          reasonCode: settlement.reasonCode,
          lastError: settlement.lastError,
          updatedAt: settlement.settledAt,
        };
        return record;
      },
      async listTelegramOperatorConsults() {
        return options.listed ?? [];
      },
    },
    telegramConnector: options.connector,
    now: () => "2026-08-13T20:00:01Z",
    recentEvents: [],
  };
  return { state, settlements, record: () => record };
}

const sentMessages: NormalizedChannelOutboundMessage[] = [];
const success = harness({
  connector: {
    async sendOutbound(message) {
      sentMessages.push(message);
      return { attempts: 2, externalMessageIds: ["telegram-message-10"] };
    },
  },
});
const successReceipt = await createServiceTelegramConsultRuntime(
  () => success.state as never,
).request({
  caller: {
    type: "direct_brain",
    sessionId: "session-1",
    wakeId: "wake-1",
    toolCallId: "call-1",
  },
  message: baseRecord.body,
  category: "network_trouble",
  originatingWakeKind: "operator",
});
assert.equal(successReceipt.status, "sent");
assert.equal(sentMessages.length, 1);
assert.equal(sentMessages[0]?.replyToExternalMessageId, undefined);
assert.equal(sentMessages[0]?.providerRefs.externalChannelId, "-100500");
assert.equal(sentMessages[0]?.providerRefs.externalThreadId, "42");
assert.deepEqual(success.settlements[0]?.externalMessageIds, [
  "telegram-message-10",
]);

const disabled = harness({});
const disabledReceipt = await createServiceTelegramConsultRuntime(
  () => disabled.state as never,
).request({
  caller: {
    type: "direct_brain",
    sessionId: "session-1",
    wakeId: "wake-1",
    toolCallId: "call-1",
  },
  message: baseRecord.body,
});
assert.equal(disabledReceipt.ok, false);
assert.equal(disabledReceipt.status, "failed");
assert.equal(
  disabled.settlements[0]?.reasonCode,
  "telegram_connector_unavailable",
);

const failed = harness({
  connector: {
    async sendOutbound() {
      throw new Error("retry budget exhausted");
    },
  },
});
const failedReceipt = await createServiceTelegramConsultRuntime(
  () => failed.state as never,
).request({
  caller: {
    type: "direct_brain",
    sessionId: "session-1",
    wakeId: "wake-1",
    toolCallId: "call-1",
  },
  message: baseRecord.body,
});
assert.equal(failedReceipt.ok, false);
assert.equal(
  failed.settlements[0]?.reasonCode,
  "telegram_operator_consult_delivery_failed",
);

const recoveredMessages: NormalizedChannelOutboundMessage[] = [];
const recovered = harness({
  listed: [{ ...baseRecord }],
  connector: {
    async sendOutbound(message) {
      recoveredMessages.push(message);
      return { attempts: 1, externalMessageIds: ["recovered-message"] };
    },
  },
});
await recoverPendingTelegramOperatorConsults(recovered.state as never);
assert.equal(recoveredMessages.length, 1);
assert.equal(recovered.record().status, "sent");

console.log("Telegram consult hosted runtime smoke passed");
