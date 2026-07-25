import type { BrainToolResult } from "./brain-tool.js";

export interface BrainToolMediaReference {
  attachmentId: string;
  filename: string;
  mimeType: string;
  byteSize: number;
  width: number;
  height: number;
  downloadUrl: string;
}

export interface BrainToolMediaSink {
  persistImages(input: {
    sessionId: string;
    wakeId: string;
    callId: string;
    toolName: string;
    result: BrainToolResult;
  }): Promise<readonly BrainToolMediaReference[]>;
}
