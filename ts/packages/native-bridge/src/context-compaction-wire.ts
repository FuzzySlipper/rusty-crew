import type {
  ContextCompactionArtifact,
  ManualContextCompactionRequest,
  ManualContextCompactionResponse,
} from "@rusty-crew/contracts";

export interface RawManualContextCompactionRequest {
  session_id: string;
  intent_key?: string | null;
  strategy_id?: string | null;
  strategy_revision?: string | null;
  source_projection_fingerprint?: string | null;
  expect_revision?: number | null;
}

export interface RawManualContextCompactionResponse {
  artifact: ContextCompactionArtifact;
  terminal_status: string;
  idempotent: boolean;
  revision: number;
}

export function toRawManualContextCompactionRequest(
  request: ManualContextCompactionRequest,
): RawManualContextCompactionRequest {
  return {
    session_id: request.sessionId,
    intent_key: request.intentKey,
    strategy_id: request.strategyId,
    strategy_revision: request.strategyRevision,
    source_projection_fingerprint: request.sourceProjectionFingerprint,
    expect_revision: request.expectRevision,
  };
}

export function toManualContextCompactionResponse(
  response: RawManualContextCompactionResponse,
): ManualContextCompactionResponse {
  return {
    artifact: response.artifact,
    terminalStatus: response.terminal_status,
    idempotent: response.idempotent,
    revision: response.revision,
  };
}
