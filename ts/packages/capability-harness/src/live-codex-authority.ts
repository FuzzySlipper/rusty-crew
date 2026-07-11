import {
  CODEX_APP_SERVER_PROTOCOL,
  type CodexControllerAuthority,
  type CodexProtocolFault,
  type CodexServerRequestContext,
  type NeutralExternalRuntimeEvent,
  type ServerRequestResolution,
} from "@rusty-crew/external-runtime-codex";

export interface RecordedCodexInteraction extends Record<string, unknown> {
  method: string;
  transportSequence: number;
  nativeRequestId?: string | number;
  resolutionType: ServerRequestResolution["type"];
}

export type CodexServerRequestResolver = (
  context: CodexServerRequestContext,
) => Promise<ServerRequestResolution> | ServerRequestResolution;

export class RecordingCodexAuthority implements CodexControllerAuthority {
  readonly events: NeutralExternalRuntimeEvent[] = [];
  readonly faults: CodexProtocolFault[] = [];
  readonly interactions: RecordedCodexInteraction[] = [];

  constructor(
    private readonly resolver: CodexServerRequestResolver = (context) => ({
      type: "error",
      code: -32000,
      message: `capability harness does not permit ${context.request.method}`,
    }),
  ) {}

  async authorizeHandshake(identity: {
    userAgent: string;
    codexHome: string;
  }): Promise<{ accepted: boolean; message?: string }> {
    const accepted =
      identity.userAgent.includes(CODEX_APP_SERVER_PROTOCOL.cliVersion) &&
      identity.codexHome.length > 0;
    return accepted
      ? { accepted }
      : { accepted, message: `unexpected identity ${identity.userAgent}` };
  }

  hasControllerLease(): boolean {
    return true;
  }

  onEvent(event: NeutralExternalRuntimeEvent): void {
    this.events.push(event);
  }

  async resolveServerRequest(
    context: CodexServerRequestContext,
  ): Promise<ServerRequestResolution> {
    const resolution = await this.resolver(context);
    this.interactions.push({
      method: context.request.method,
      transportSequence: context.transportSequence,
      ...(context.request.id === undefined
        ? {}
        : { nativeRequestId: context.request.id }),
      resolutionType: resolution.type,
    });
    return resolution;
  }

  onProtocolFault(fault: CodexProtocolFault): void {
    this.faults.push(fault);
  }

  onDisconnected(): void {}
}
