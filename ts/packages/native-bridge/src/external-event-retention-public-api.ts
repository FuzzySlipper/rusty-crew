export interface NativeRuntimeExternalEventStorageDiagnostics {
  eventRows: number;
  estimatedEventBytes: number;
  checkpointRows: number;
  oldestSequence?: number;
  oldestCreatedAt?: string;
  newestSequence?: number;
  newestCreatedAt?: string;
}

export interface NativeRuntimeFilesystemHeadroom {
  available: boolean;
  source: string;
  path?: string;
  totalBytes?: number;
  freeBytes?: number;
  freePercent?: number;
  warningFreePercent?: number;
  warningActive: boolean;
  detail: string;
}

export interface NativeRuntimeMaintenancePolicy {
  expireQueuedMessagesAt?: string;
  purgeTerminalQueuedMessagesBefore?: string;
  expireProviderWireStatesAt?: string;
  compactSessionMemoryAt?: string;
  sessionMemoryMaxActiveRecordsPerScope?: number;
  sessionMemoryArchiveBatchSize?: number;
  compactTerminalExternalRuntimeEventsBefore?: string;
  externalRuntimeEventRetentionAt?: string;
  externalRuntimeEventTerminalTurnBatchSize?: number;
  runWalCheckpoint?: boolean;
  runOptimize?: boolean;
}

export interface NativeExternalRuntimeEventRetentionReport {
  enabled: boolean;
  cutoff?: string;
  terminalTurnBatchSize?: number;
  terminalTurnsInspected: number;
  terminalTurnsCompacted: number;
  checkpointsCreated: number;
  eventsDeleted: number;
  estimatedReclaimedBytes: number;
  oldestRetainedSequence?: number;
  oldestRetainedAt?: string;
}
