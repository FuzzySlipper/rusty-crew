import type {
  ExternalControllerContext,
  ExternalTurnCorrelation,
  ExternalTurnPhase,
} from "@rusty-crew/contracts";

export interface NativeExternalRuntimeTurnBridgeMethods {
  getExternalTurn(
    requestId: string,
  ): Promise<ExternalTurnCorrelation | undefined>;
  listExternalTurnsForNativeThread(
    runtimeId: string,
    nativeThreadId: string,
  ): Promise<ExternalTurnCorrelation[]>;
  listActiveExternalTurns(): Promise<ExternalTurnCorrelation[]>;
  expireExternalTurnDispatches(now: string): Promise<ExternalTurnCorrelation[]>;
  transitionExternalTurn(input: {
    controller: ExternalControllerContext;
    requestId: string;
    nextPhase: ExternalTurnPhase;
    nativeTurnId?: string;
    terminalReasonCode?: string;
    terminalError?: ExternalTurnCorrelation["terminalError"];
    now: string;
  }): Promise<ExternalTurnCorrelation>;
}
