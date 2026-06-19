import type {
  DenDataUpdate,
  EngineConfig,
  EventReceipt,
  ExternalEvent,
} from "@rusty-crew/contracts";
import {
  loadNativeBridge,
  type NativeBridgeModule,
} from "@rusty-crew/native-bridge";

export class CoreBridge {
  private constructor(
    readonly manifestVersion: number,
    private readonly native: NativeBridgeModule,
  ) {}

  static async initialize(_config: EngineConfig): Promise<CoreBridge> {
    const native = await loadNativeBridge();
    return new CoreBridge(native.manifestVersion, native);
  }

  async injectDenDataUpdate(update: DenDataUpdate): Promise<EventReceipt> {
    if (!this.native.injectDenDataUpdate) {
      throw new Error(
        "native bridge does not implement inject_den_data_update",
      );
    }

    return this.native.injectDenDataUpdate(update);
  }

  async injectExternalEvent(event: ExternalEvent): Promise<EventReceipt> {
    if (!this.native.injectExternalEvent) {
      throw new Error("native bridge does not implement inject_external_event");
    }

    return this.native.injectExternalEvent(event);
  }
}

export * from "@rusty-crew/contracts";
