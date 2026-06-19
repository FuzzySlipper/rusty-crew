import type { EngineConfig } from "@rusty-crew/contracts";
import { loadNativeBridge } from "@rusty-crew/native-bridge";

export class CoreBridge {
  private constructor(readonly manifestVersion: number) {}

  static async initialize(_config: EngineConfig): Promise<CoreBridge> {
    const native = await loadNativeBridge();
    return new CoreBridge(native.manifestVersion);
  }
}

export * from "@rusty-crew/contracts";
