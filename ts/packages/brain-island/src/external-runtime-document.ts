import type {
  ExternalRuntimeDocumentCaptureCandidate,
  ExternalRuntimeDocumentProjection,
} from "@rusty-crew/external-runtime-codex";

export type ExternalRuntimeDocumentCaptureState =
  | "available"
  | "missing"
  | "binary"
  | "empty"
  | "oversized"
  | "changed"
  | "unsupported"
  | "failed";

export interface ExternalRuntimeDocumentReference extends ExternalRuntimeDocumentProjection {
  readonly captureState: ExternalRuntimeDocumentCaptureState;
}

export interface ExternalRuntimeDocumentCaptureInput {
  readonly runtimeId: string;
  readonly sessionId?: string;
  readonly bindingId?: string;
  readonly nativeThreadId?: string;
  readonly nativeTurnId?: string;
  readonly itemId?: string;
  readonly externalEventId: string;
  readonly candidates: readonly ExternalRuntimeDocumentCaptureCandidate[];
}

export interface ExternalRuntimeDocumentCaptureSink {
  captureExternalRuntimeDocuments(
    input: ExternalRuntimeDocumentCaptureInput,
  ): Promise<readonly ExternalRuntimeDocumentReference[]>;
}
