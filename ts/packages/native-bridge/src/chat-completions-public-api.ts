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
