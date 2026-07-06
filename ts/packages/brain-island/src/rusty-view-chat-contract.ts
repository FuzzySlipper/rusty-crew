export const RUSTY_VIEW_CHAT_CONTRACT_VERSION = "0.1.0";

export const RUSTY_VIEW_CHAT_OPENAPI_PATH =
  "docs/rusty-view-chat-api-v0.openapi.json";

export const RUSTY_VIEW_CHAT_PATHS = {
  sessions: "/v1/chat/sessions",
  session: "/v1/chat/sessions/{session_id}",
  events: "/v1/chat/sessions/{session_id}/events",
  context: "/v1/chat/sessions/{session_id}/context",
  stream: "/v1/chat/sessions/{session_id}/stream",
  toolCallDebug: "/v1/chat/sessions/{session_id}/tool-calls/{debug_detail_id}",
  providerRequestDebug:
    "/v1/chat/sessions/{session_id}/provider-requests/{debug_detail_id}",
  messages: "/v1/chat/sessions/{session_id}/messages",
  slots: "/v1/chat/sessions/{session_id}/slots",
  slotVariants: "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants",
  slotVariant:
    "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/{variant_id}",
  reorderSlotVariants:
    "/v1/chat/sessions/{session_id}/slots/{slot_id}/variants/reorder",
  activeSlotVariant:
    "/v1/chat/sessions/{session_id}/slots/{slot_id}/active-variant",
  tree: "/v1/chat/sessions/{session_id}/tree",
  jump: "/v1/chat/sessions/{session_id}/jump",
  sessionSearch: "/v1/chat/sessions/{session_id}/search",
  search: "/v1/chat/search",
  branches: "/v1/chat/sessions/{session_id}/branches",
  activeBranch: "/v1/chat/sessions/{session_id}/branches/active",
  branchHead: "/v1/chat/sessions/{session_id}/branches/{branch_id}/head",
  snapshots: "/v1/chat/sessions/{session_id}/snapshots",
  attachments: "/v1/chat/sessions/{session_id}/attachments",
  attachment: "/v1/chat/sessions/{session_id}/attachments/{attachment_id}",
  dataBankScopes: "/v1/chat/sessions/{session_id}/data-bank/scopes",
  dataBankScope: "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}",
  dataBankScopeAttachments:
    "/v1/chat/sessions/{session_id}/data-bank/scopes/{scope_id}/attachments",
  commands: "/v1/chat/commands",
  commandAutocomplete: "/v1/chat/commands/{command_name}/autocomplete",
  sessionCommands: "/v1/chat/sessions/{session_id}/commands",
} as const;

export const RUSTY_VIEW_CHAT_EVENT_KIND_VALUES = [
  "session_snapshot",
  "message_created",
  "assistant_turn_started",
  "assistant_text_delta",
  "assistant_reasoning_delta",
  "phase_change",
  "provider_status",
  "assistant_message_completed",
  "assistant_turn_finished",
  "tool_call_started",
  "tool_call_completed",
  "tool_call_failed",
  "command_started",
  "command_completed",
  "command_failed",
  "context_status",
  "context_compaction_started",
  "context_compaction_completed",
  "context_compaction_failed",
  "message_slot_created",
  "message_variant_created",
  "message_variant_deleted",
  "message_variants_reordered",
  "message_active_variant_selected",
  "conversation_branch_created",
  "conversation_active_branch_selected",
  "conversation_branch_head_updated",
  "conversation_snapshot_created",
  "attachment_uploaded",
  "attachment_linked",
  "attachment_removed",
  "attachment_updated",
  "data_bank_scope_created",
  "data_bank_scope_removed",
  "stream_error",
  "unknown",
] as const;

export const RUSTY_VIEW_CHAT_EVENT_REQUIRED_FIELDS = [
  "event_id",
  "session_id",
  "sequence_id",
  "created_at",
  "kind",
  "payload",
] as const;

export const RUSTY_VIEW_MESSAGE_SLOT_REQUIRED_FIELDS = [
  "slot_id",
  "session_id",
  "primary_variant_id",
  "metadata_json",
  "created_at",
  "updated_at",
  "version",
  "primary",
  "alternates",
] as const;
