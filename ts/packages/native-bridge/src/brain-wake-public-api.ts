import type {
  BrainWakeRequest,
  RuntimeBufferHandle,
  RuntimeBufferView,
  SessionExecutionState,
  SessionState,
  Unit,
} from "@rusty-crew/contracts";

import type {
  BrainWakeExecutionResult,
  NativeChatEventLogEvent,
  NativeChatReadModelPage,
  NativeExactPage,
} from "./public-api.js";

export interface NativeChatSessionReadResult {
  session: SessionState;
  execution: SessionExecutionState;
  events: NativeChatEventLogEvent[];
  latest_cursor: string;
  has_more: boolean;
  has_more_before: boolean;
  total: number;
  message_count: number;
  source: NativeChatReadModelPage["source"];
  message_slots: NativeExactPage<unknown>;
}

export interface BridgeBufferClient {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
  readChatSession?(input: unknown): Promise<NativeChatSessionReadResult>;
}

export interface BrainWakeExecutor {
  wake(
    request: BrainWakeRequest,
    buffers: BridgeBufferClient,
    options?: { signal?: AbortSignal },
  ): Promise<BrainWakeExecutionResult> | BrainWakeExecutionResult;
}
