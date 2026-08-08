export interface NativeBrainContextCompactionPolicy {
  enabled: boolean;
  autoCompactionEnabled: boolean;
  strategyId: string;
  contextWindowTokens: number;
  compactAtPercent: number;
  targetPercentAfterCompaction: number;
}
