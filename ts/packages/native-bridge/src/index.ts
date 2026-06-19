export interface NativeBridgeModule {
  readonly manifestVersion: number;
}

export async function loadNativeBridge(): Promise<NativeBridgeModule> {
  return { manifestVersion: 1 };
}
