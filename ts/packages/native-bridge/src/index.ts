import type {
  DenDataUpdate,
  EventReceipt,
  ExternalEvent,
} from "@rusty-crew/contracts";

export interface NativeBridgeModule {
  readonly manifestVersion: number;
  injectDenDataUpdate?(update: DenDataUpdate): Promise<EventReceipt>;
  injectExternalEvent?(event: ExternalEvent): Promise<EventReceipt>;
}

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  return { manifestVersion: 1 };
}
