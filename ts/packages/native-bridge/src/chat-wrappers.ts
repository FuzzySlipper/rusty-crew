import { validateBridgeValue } from "./bridge-validation.js";
import {
  chatEventLogEventSchema,
  chatEventLogPageSchema,
  chatReadModelPageSchema,
} from "./bridge-validation-schemas.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeBridgeModule,
  NativeChatEventLogEvent,
  NativeChatEventLogPage,
  NativeChatReadModelPage,
  NativeChatSessionReadResult,
  NativeChatSessionSummaryPage,
  NativeExactPage,
} from "./public-api.js";
import { toSessionState, type RawSessionState } from "./session-wire.js";

type ChatMethodName =
  | "saveMessageSlot"
  | "saveMessageVariant"
  | "createChatMessageSlot"
  | "createChatMessageVariant"
  | "applyRoleplayAlternative"
  | "chatReadModelPage"
  | "readChatSession"
  | "queryChatSessionSummaries"
  | "appendChatEvent"
  | "queryChatEvents"
  | "queryMessageSlots"
  | "queryMessageSlotsPage"
  | "queryMessageVariants"
  | "queryMessageVariantsPage"
  | "selectActiveMessageVariant"
  | "selectActiveChatMessageVariant"
  | "deleteChatMessageVariant"
  | "reorderChatMessageVariants"
  | "deleteMessageVariant"
  | "reorderMessageVariants"
  | "saveConversationBranch"
  | "createChatConversationBranch"
  | "ensureActiveChatConversationBranch"
  | "queryConversationBranches"
  | "getConversationBranchState"
  | "selectActiveConversationBranch"
  | "updateConversationBranchHead"
  | "saveConversationSnapshot"
  | "createChatConversationSnapshot"
  | "queryConversationSnapshots"
  | "readConversationTree"
  | "searchChatTranscript"
  | "resolveConversationJump"
  | "saveAttachment"
  | "createChatAttachment"
  | "queryAttachments"
  | "queryAttachmentsPage"
  | "removeAttachment"
  | "removeChatAttachment"
  | "saveDataBankScope"
  | "createChatDataBankScope"
  | "queryDataBankScopes"
  | "queryDataBankScopesPage"
  | "removeDataBankScope"
  | "removeChatDataBankScope";

interface RawChatSessionReadFacts {
  session: RawSessionState;
  message_count: number;
  latest_cursor: string;
  source: NativeChatReadModelPage["source"];
}

interface RawChatSessionSummaryPage {
  page: NativeExactPage<RawChatSessionReadFacts>;
}

interface RawChatSessionReadResult {
  session: RawSessionState;
  events: NativeChatEventLogEvent[];
  latest_cursor: string;
  has_more: boolean;
  has_more_before: boolean;
  total: number;
  message_count: number;
  source: NativeChatReadModelPage["source"];
  message_slots: NativeExactPage<unknown>;
}

export function createNativeBridgeChatMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, ChatMethodName> {
  return {
    saveMessageSlot: async (input) =>
      binding.saveMessageSlotJson(JSON.stringify(input)),
    saveMessageVariant: async (input) =>
      JSON.parse(
        binding.saveMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    createChatMessageSlot: async (input) =>
      JSON.parse(
        binding.createChatMessageSlotJson(JSON.stringify(input)),
      ) as unknown,
    createChatMessageVariant: async (input) =>
      JSON.parse(
        binding.createChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    applyRoleplayAlternative: async (input) =>
      JSON.parse(
        binding.applyRoleplayAlternativeJson(JSON.stringify(input)),
      ) as unknown,
    chatReadModelPage: async (input) =>
      validateBridgeValue<NativeChatReadModelPage>({
        operation: "chat_read_model_page",
        direction: "rust_to_ts",
        schema: chatReadModelPageSchema,
        value: JSON.parse(
          binding.chatReadModelPageJson(JSON.stringify(input)),
        ) as unknown,
      }),
    readChatSession: async (input) =>
      toNativeChatSessionReadResult(
        JSON.parse(
          binding.readChatSessionJson(JSON.stringify(input)),
        ) as RawChatSessionReadResult,
      ),
    queryChatSessionSummaries: async (input) =>
      toNativeChatSessionSummaryPage(
        JSON.parse(
          binding.queryChatSessionSummariesJson(JSON.stringify(input)),
        ) as RawChatSessionSummaryPage,
      ),
    appendChatEvent: async (input) =>
      validateBridgeValue<NativeChatEventLogEvent>({
        operation: "append_chat_event",
        direction: "rust_to_ts",
        schema: chatEventLogEventSchema,
        value: JSON.parse(
          binding.appendChatEventJson(JSON.stringify(input)),
        ) as unknown,
      }),
    queryChatEvents: async (input) =>
      validateBridgeValue<NativeChatEventLogPage>({
        operation: "query_chat_events",
        direction: "rust_to_ts",
        schema: chatEventLogPageSchema,
        value: JSON.parse(
          binding.queryChatEventsJson(JSON.stringify(input)),
        ) as unknown,
      }),
    queryMessageSlots: async (query) =>
      JSON.parse(
        binding.queryMessageSlotsJson(JSON.stringify(query)),
      ) as unknown[],
    queryMessageSlotsPage: async (query) =>
      JSON.parse(
        binding.queryMessageSlotsPageJson(JSON.stringify(query)),
      ) as NativeExactPage<unknown>,
    queryMessageVariants: async (query) =>
      JSON.parse(
        binding.queryMessageVariantsJson(JSON.stringify(query)),
      ) as unknown[],
    queryMessageVariantsPage: async (query) =>
      JSON.parse(
        binding.queryMessageVariantsPageJson(JSON.stringify(query)),
      ) as NativeExactPage<unknown>,
    selectActiveMessageVariant: async (input) =>
      JSON.parse(
        binding.selectActiveMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    selectActiveChatMessageVariant: async (input) =>
      JSON.parse(
        binding.selectActiveChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    deleteChatMessageVariant: async (input) =>
      JSON.parse(
        binding.deleteChatMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    reorderChatMessageVariants: async (input) =>
      JSON.parse(
        binding.reorderChatMessageVariantsJson(JSON.stringify(input)),
      ) as unknown[],
    deleteMessageVariant: async (input) =>
      JSON.parse(
        binding.deleteMessageVariantJson(JSON.stringify(input)),
      ) as unknown,
    reorderMessageVariants: async (input) =>
      JSON.parse(
        binding.reorderMessageVariantsJson(JSON.stringify(input)),
      ) as unknown[],
    saveConversationBranch: async (input) =>
      JSON.parse(
        binding.saveConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    createChatConversationBranch: async (input) =>
      JSON.parse(
        binding.createChatConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    ensureActiveChatConversationBranch: async (input) =>
      JSON.parse(
        binding.ensureActiveChatConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    queryConversationBranches: async (query) =>
      JSON.parse(
        binding.queryConversationBranchesJson(JSON.stringify(query)),
      ) as unknown[],
    getConversationBranchState: async (input) =>
      JSON.parse(
        binding.getConversationBranchStateJson(JSON.stringify(input)),
      ) as unknown,
    selectActiveConversationBranch: async (input) =>
      JSON.parse(
        binding.selectActiveConversationBranchJson(JSON.stringify(input)),
      ) as unknown,
    updateConversationBranchHead: async (input) =>
      JSON.parse(
        binding.updateConversationBranchHeadJson(JSON.stringify(input)),
      ) as unknown,
    saveConversationSnapshot: async (input) =>
      JSON.parse(
        binding.saveConversationSnapshotJson(JSON.stringify(input)),
      ) as unknown,
    createChatConversationSnapshot: async (input) =>
      JSON.parse(
        binding.createChatConversationSnapshotJson(JSON.stringify(input)),
      ) as unknown,
    queryConversationSnapshots: async (query) =>
      JSON.parse(
        binding.queryConversationSnapshotsJson(JSON.stringify(query)),
      ) as unknown[],
    readConversationTree: async (query) =>
      JSON.parse(
        binding.readConversationTreeJson(JSON.stringify(query)),
      ) as unknown,
    searchChatTranscript: async (query) =>
      JSON.parse(
        binding.searchChatTranscriptJson(JSON.stringify(query)),
      ) as unknown,
    resolveConversationJump: async (input) =>
      JSON.parse(
        binding.resolveConversationJumpJson(JSON.stringify(input)),
      ) as unknown,
    saveAttachment: async (input) =>
      JSON.parse(binding.saveAttachmentJson(JSON.stringify(input))) as unknown,
    createChatAttachment: async (input) =>
      JSON.parse(
        binding.createChatAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    queryAttachments: async (query) =>
      JSON.parse(
        binding.queryAttachmentsJson(JSON.stringify(query)),
      ) as unknown[],
    queryAttachmentsPage: async (query) =>
      JSON.parse(
        binding.queryAttachmentsPageJson(JSON.stringify(query)),
      ) as NativeExactPage<unknown>,
    removeAttachment: async (input) =>
      JSON.parse(
        binding.removeAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    removeChatAttachment: async (input) =>
      JSON.parse(
        binding.removeChatAttachmentJson(JSON.stringify(input)),
      ) as unknown,
    saveDataBankScope: async (input) =>
      JSON.parse(
        binding.saveDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    createChatDataBankScope: async (input) =>
      JSON.parse(
        binding.createChatDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    queryDataBankScopes: async (query) =>
      JSON.parse(
        binding.queryDataBankScopesJson(JSON.stringify(query)),
      ) as unknown[],
    queryDataBankScopesPage: async (query) =>
      JSON.parse(
        binding.queryDataBankScopesPageJson(JSON.stringify(query)),
      ) as NativeExactPage<unknown>,
    removeDataBankScope: async (input) =>
      JSON.parse(
        binding.removeDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
    removeChatDataBankScope: async (input) =>
      JSON.parse(
        binding.removeChatDataBankScopeJson(JSON.stringify(input)),
      ) as unknown,
  };
}

function toNativeChatSessionReadResult(
  raw: RawChatSessionReadResult,
): NativeChatSessionReadResult {
  return {
    session: toSessionState(raw.session),
    events: raw.events,
    latest_cursor: raw.latest_cursor,
    has_more: raw.has_more,
    has_more_before: raw.has_more_before,
    total: raw.total,
    message_count: raw.message_count,
    source: raw.source,
    message_slots: raw.message_slots,
  };
}

function toNativeChatSessionSummaryPage(
  raw: RawChatSessionSummaryPage,
): NativeChatSessionSummaryPage {
  return {
    page: {
      ...raw.page,
      items: raw.page.items.map((facts) => ({
        ...facts,
        session: toSessionState(facts.session),
      })),
    },
  };
}
