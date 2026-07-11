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
  readonly reasonCode?: string;
  readonly message?: string;
}

export interface BoundedRawDetail {
  readonly json: string;
  readonly originalSha256: string;
  readonly truncated: boolean;
  readonly redactedKeys: readonly string[];
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
  readonly message?: string;
  readonly command?: string;
  readonly cwd?: string;
  readonly output?: string;
  readonly exitCode?: number;
  readonly durationMs?: number;
  readonly server?: string;
  readonly tool?: string;
  readonly success?: boolean;
  readonly summary?: readonly string[];
  readonly fileChanges?: readonly {
    readonly path?: string;
    readonly kind?: string;
    readonly status?: string;
  }[];
  readonly usage?: Readonly<Record<string, number>>;
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
  readonly maxPendingRequests?: number;
  readonly maxRawDetailBytes?: number;
  readonly clientName?: string;
  readonly clientTitle?: string;
  readonly clientVersion?: string;
}
