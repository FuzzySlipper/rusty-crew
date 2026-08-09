import type {
  ExternalRuntimeMediaCaptureCandidate,
  ExternalRuntimeMediaProjection,
} from "@rusty-crew/external-runtime-codex";

export type ExternalRuntimeMediaCaptureState =
  | "available"
  | "unavailable"
  | "unsupported"
  | "empty"
  | "oversized"
  | "failed";

export interface ExternalRuntimeMediaReference extends ExternalRuntimeMediaProjection {
  readonly captureState: ExternalRuntimeMediaCaptureState;
}

export interface ExternalRuntimeMediaCaptureInput {
  readonly runtimeId: string;
  readonly sessionId?: string;
  readonly bindingId?: string;
  readonly nativeThreadId?: string;
  readonly nativeTurnId?: string;
  readonly itemId?: string;
  readonly externalEventId: string;
  readonly toolName?: string;
  readonly candidates: readonly ExternalRuntimeMediaCaptureCandidate[];
}

export interface ExternalRuntimeMediaCaptureSink {
  captureExternalRuntimeMedia(
    input: ExternalRuntimeMediaCaptureInput,
  ): Promise<readonly ExternalRuntimeMediaReference[]>;
}
