import type { ServerRequest } from "../protocol/0.144.1/ts/ServerRequest.js";

export type JsonRpcId = string | number;

export interface CodexProtocolIdentity {
  readonly protocol: "codex-app-server";
  readonly cliVersion: string;
  readonly experimental: true;
  readonly launcherSha256: string;
  readonly nativeExecutableSha256: string;
  readonly tsSha256: string;
  readonly jsonSchemaSha256: string;
  readonly protocolSchemaSha256: string;
}

export interface CodexInitializeIdentity {
  readonly userAgent: string;
  readonly codexHome: string;
  readonly platformFamily: string;
  readonly platformOs: string;
  readonly protocol: CodexProtocolIdentity;
}

export interface ControllerAuthorization {
  readonly accepted: boolean;
  readonly retryable?: boolean;
  readonly reasonCode?: string;
  readonly message?: string;
}

export type CodexCompatibilityProbeOutcome =
  | "passed"
  | "transport_retryable"
  | "incompatible";

export interface CodexCompatibilityProbeStep {
  readonly stepId: string;
  readonly status: "passed" | "skipped" | "failed";
  readonly durationMs: number;
  readonly reasonCode?: string;
  readonly detail?: string;
}

export interface CodexCompatibilityProbeReport {
  readonly suiteRevision: string;
  readonly outcome: CodexCompatibilityProbeOutcome;
  readonly steps: CodexCompatibilityProbeStep[];
  readonly completedAt: string;
}

export interface BoundedRawDetail {
  readonly json: string;
  readonly originalSha256: string;
  readonly truncated: boolean;
  readonly redactedKeys: readonly string[];
}

/**
 * Transient media input captured from a native Codex notification. These
 * values are consumed by the Crew service before the normalized event is
 * persisted and must never be exposed through the public event DTO.
 */
export type ExternalRuntimeMediaCaptureCandidate =
  | {
      readonly source: "dynamic_tool_input_image";
      readonly mediaIndex: number;
      readonly imageUrl: string;
    }
  | {
      readonly source: "mcp_image_content";
      readonly mediaIndex: number;
      readonly data: string;
      readonly mimeType: string;
    }
  | {
      readonly source: "image_view_path";
      readonly mediaIndex: number;
      readonly path: string;
    };

/**
 * A host file deliberately presented to the operator through a Markdown link
 * in an agent message. The path is transient capture input and is never part
 * of the public normalized event.
 */
export interface ExternalRuntimeDocumentCaptureCandidate {
  readonly source: "agent_message_file_link";
  readonly documentIndex: number;
  readonly path: string;
  readonly displayName: string;
}

export interface ExternalRuntimeDocumentProjection {
  readonly documentIndex: number;
  readonly captureSource: ExternalRuntimeDocumentCaptureCandidate["source"];
  readonly captureState:
    | "available"
    | "missing"
    | "binary"
    | "empty"
    | "oversized"
    | "changed"
    | "unsupported"
    | "failed";
  readonly reasonCode?: string;
  readonly attachmentId?: string;
  readonly filename?: string;
  readonly mimeType?: string;
  readonly languageHint?: string;
  readonly byteSize?: number;
  readonly sha256?: string;
  readonly contentUrl?: string;
}

export interface ExternalRuntimeMediaProjection {
  readonly mediaIndex: number;
  readonly captureSource: ExternalRuntimeMediaCaptureCandidate["source"];
  readonly captureState:
    | "available"
    | "unavailable"
    | "unsupported"
    | "empty"
    | "oversized"
    | "failed";
  readonly reasonCode?: string;
  readonly attachmentId?: string;
  readonly filename?: string;
  readonly mimeType?: string;
  readonly byteSize?: number;
  readonly sha256?: string;
  readonly width?: number;
  readonly height?: number;
  readonly contentUrl?: string;
}

export type NeutralExternalEventKind =
  | "thread_lifecycle"
  | "turn_lifecycle"
  | "item_lifecycle"
  | "assistant_text_delta"
  | "reasoning_delta"
  | "plan_delta"
  | "command_activity"
  | "file_activity"
  | "mcp_activity"
  | "dynamic_tool_activity"
  | "usage"
  | "compaction"
  | "runtime_warning"
  | "runtime_status"
  | "unknown_native_notification"
  | "unsupported_server_request";

export interface NeutralExternalRuntimeEventPayload {
  readonly nativeMethod: string;
  readonly status?: string;
  readonly text?: string;
  readonly messagePhase?: "commentary" | "final_answer" | "unknown";
  readonly message?: string;
  readonly error?: {
    readonly message: string;
    readonly code: string | null;
    readonly additionalDetails: string | null;
    readonly willRetry: boolean;
  };
  readonly command?: string;
  readonly cwd?: string;
  readonly output?: string;
  readonly exitCode?: number;
  readonly durationMs?: number;
  readonly server?: string;
  readonly tool?: string;
  readonly success?: boolean;
  readonly media?: readonly ExternalRuntimeMediaProjection[];
  readonly documents?: readonly ExternalRuntimeDocumentProjection[];
  readonly summary?: readonly string[];
  readonly fileChanges?: readonly {
    readonly path?: string;
    readonly kind?: string;
    readonly status?: string;
  }[];
  readonly settings?: {
    readonly model: string;
    readonly modelProvider: string;
    readonly effort: string | null;
  };
  readonly usage?: {
    readonly total: Readonly<Record<string, number>>;
    readonly last: Readonly<Record<string, number>>;
    readonly modelContextWindow: number | null;
  };
}

export interface NeutralExternalRuntimeEvent {
  readonly transportSequence: number;
  readonly method: string;
  readonly kind: NeutralExternalEventKind;
  readonly threadId?: string;
  readonly turnId?: string;
  readonly itemId?: string;
  readonly nativeRequestId?: JsonRpcId;
  readonly payload: NeutralExternalRuntimeEventPayload;
  readonly mediaCandidates?: readonly ExternalRuntimeMediaCaptureCandidate[];
  readonly documentCandidates?: readonly ExternalRuntimeDocumentCaptureCandidate[];
  readonly rawDetail: BoundedRawDetail;
}

export type ServerRequestResolution =
  | { readonly type: "success"; readonly result: unknown }
  | {
      readonly type: "error";
      readonly code: number;
      readonly message: string;
      readonly data?: unknown;
    };

export interface CodexServerRequestContext {
  readonly transportSequence: number;
  readonly request: ServerRequest;
  readonly rawDetail: BoundedRawDetail;
}

export interface CodexProtocolFault {
  readonly reasonCode:
    | "malformed_message"
    | "malformed_known_notification"
    | "malformed_known_request"
    | "malformed_response"
    | "malformed_server_request_resolution"
    | "duplicate_response"
    | "unknown_response"
    | "transport_error";
  readonly message: string;
  readonly fatal: boolean;
  readonly rawDetail?: BoundedRawDetail;
}

export interface CodexControllerAuthority {
  authorizeHandshake(
    identity: CodexInitializeIdentity,
    probeReport: CodexCompatibilityProbeReport,
  ): Promise<ControllerAuthorization>;
  hasControllerLease(): boolean | Promise<boolean>;
  onEvent(event: NeutralExternalRuntimeEvent): void | Promise<void>;
  resolveServerRequest(
    context: CodexServerRequestContext,
  ): Promise<ServerRequestResolution>;
  onProtocolFault(fault: CodexProtocolFault): void | Promise<void>;
  onDisconnected(details: {
    readonly reason: string;
    readonly pendingClientRequestIds: readonly JsonRpcId[];
    readonly pendingServerRequestIds: readonly JsonRpcId[];
  }): void | Promise<void>;
}

export type CodexDriverState =
  | "disconnected"
  | "connecting"
  | "ready"
  | "incompatible"
  | "closed";

export interface CodexDriverOptions {
  readonly requestTimeoutMs?: number;
  readonly compatibilityProbeTimeoutMs?: number;
  readonly maxPendingRequests?: number;
  readonly maxRawDetailBytes?: number;
  readonly clientName?: string;
  readonly clientTitle?: string;
  readonly clientVersion?: string;
}
