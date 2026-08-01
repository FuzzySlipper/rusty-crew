import type {
  RuntimeActivityBegin,
  RuntimeActivityCensus,
  RuntimeActivityCensusQuery,
  RuntimeActivityFinish,
  RuntimeActivityProgress,
  RuntimeActivityRecord,
  RuntimeActivityWakeSettlement,
} from "@rusty-crew/contracts";

export interface NativeRuntimeActivityBridgeMethods {
  beginRuntimeActivity(
    input: RuntimeActivityBegin,
  ): Promise<RuntimeActivityRecord>;
  progressRuntimeActivity(
    input: RuntimeActivityProgress,
  ): Promise<RuntimeActivityRecord>;
  finishRuntimeActivity(
    input: RuntimeActivityFinish,
  ): Promise<RuntimeActivityRecord>;
  settleRuntimeActivityWake(
    input: RuntimeActivityWakeSettlement,
  ): Promise<RuntimeActivityRecord[]>;
  runtimeActivityCensus(
    query?: RuntimeActivityCensusQuery,
  ): Promise<RuntimeActivityCensus>;
}
