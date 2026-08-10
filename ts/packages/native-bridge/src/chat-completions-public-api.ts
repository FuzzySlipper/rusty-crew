export interface ChatCompletionsChatCompletionMessage {
  role: "system" | "user" | "assistant" | "tool";
  content?: string;
  reasoningContent?: string;
  name?: string;
  toolCallId?: string;
  toolCalls?: unknown[];
}

export interface ChatCompletionsInputImage {
  attachmentId: string;
  mimeType: string;
  bytesBase64: string;
  byteSize: number;
}

export interface ChatCompletionsToolRequest {
  wakeId: string;
  callId: string;
  providerItemId?: string;
  name: string;
  argumentsJson: string;
}
export interface BrainRunCompactionDomainContext {
  compactionDomainContext?: unknown;
  compaction_domain_context?: unknown;
}
