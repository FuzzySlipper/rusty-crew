import type {
  RuntimeActivityFinish,
  RuntimeActivityWakeSettlement,
} from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";

export interface DeferredRuntimeActivitySettlement {
  wake: RuntimeActivityWakeSettlement;
  dispatch: RuntimeActivityFinish;
}

export interface RuntimeActivitySettlementReconciliation {
  reconciledWakeIds: string[];
  failure?: { wakeId: string; error: unknown };
}

export class DeferredRuntimeActivitySettlementQueue {
  readonly #pending = new Map<string, DeferredRuntimeActivitySettlement>();

  defer(settlement: DeferredRuntimeActivitySettlement): void {
    this.#pending.set(settlement.wake.wakeId, settlement);
  }

  get size(): number {
    return this.#pending.size;
  }

  async reconcile(
    bridge: Pick<
      NativeBridgeModule,
      "settleRuntimeActivityWake" | "finishRuntimeActivity"
    >,
  ): Promise<RuntimeActivitySettlementReconciliation> {
    const reconciledWakeIds: string[] = [];
    for (const [wakeId, settlement] of this.#pending) {
      try {
        await bridge.settleRuntimeActivityWake(settlement.wake);
        await bridge.finishRuntimeActivity(settlement.dispatch);
      } catch (error: unknown) {
        return { reconciledWakeIds, failure: { wakeId, error } };
      }
      this.#pending.delete(wakeId);
      reconciledWakeIds.push(wakeId);
    }
    return { reconciledWakeIds };
  }
}
