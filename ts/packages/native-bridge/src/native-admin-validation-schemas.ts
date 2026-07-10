import { Type } from "typebox";

const optionalString = Type.Optional(Type.String());
const stringArray = Type.Array(Type.String());

export const nativeRuntimeDatabaseSizeSchema = Type.Object(
  {
    databaseBytes: Type.Number(),
    pageCount: Type.Number(),
    pageSizeBytes: Type.Number(),
    freelistPages: Type.Number(),
    freelistBytes: Type.Number(),
    walBytes: Type.Number(),
  },
  { additionalProperties: false },
);

const moduleCapabilityStatusSchema = Type.Object(
  {
    capability: Type.String(),
    required: Type.Boolean(),
    supported: Type.Boolean(),
    backendVariant: optionalString,
  },
  { additionalProperties: false },
);

const moduleLogicalStoreSchema = Type.Object(
  { storeName: Type.String(), description: Type.String() },
  { additionalProperties: false },
);

const moduleNamedSchema = Type.Object(
  { name: Type.String(), description: Type.String() },
  { additionalProperties: false },
);

const modulePhysicalTableSchema = Type.Object(
  {
    tableName: Type.String(),
    logicalStore: Type.String(),
    physicalTable: Type.String(),
    declaration: Type.String(),
  },
  { additionalProperties: false },
);

const modulePhysicalIndexSchema = Type.Object(
  {
    tableName: Type.String(),
    purpose: Type.String(),
    physicalIndex: Type.String(),
    columns: stringArray,
    unique: Type.Boolean(),
  },
  { additionalProperties: false },
);

const moduleRetentionSchema = Type.Object(
  {
    storeName: Type.String(),
    policy: Type.String(),
    detail: optionalString,
  },
  { additionalProperties: false },
);

const moduleQueryCatalogSchema = Type.Object(
  {
    queryId: Type.String(),
    storeName: Type.String(),
    description: Type.String(),
    parameterSchemaId: optionalString,
  },
  { additionalProperties: false },
);

const moduleTransferHookSchema = Type.Object(
  { hookName: Type.String(), formatVersion: Type.Number() },
  { additionalProperties: false },
);

const moduleSchemaDiagnosticSchema = Type.Object(
  {
    moduleId: Type.String(),
    ownerCrate: Type.String(),
    ownerModule: Type.String(),
    descriptorVersion: Type.Number(),
    installedVersion: Type.Optional(Type.Number()),
    migrationStatus: Type.String(),
    descriptorFingerprint: Type.String(),
    installedDescriptorFingerprint: optionalString,
    installedAt: optionalString,
    updatedAt: optionalString,
    capabilityStatus: Type.Array(moduleCapabilityStatusSchema),
    logicalStores: Type.Array(moduleLogicalStoreSchema),
    physicalTables: Type.Array(modulePhysicalTableSchema),
    physicalIndexes: Type.Array(modulePhysicalIndexSchema),
    retention: Type.Array(moduleRetentionSchema),
    repositoryContracts: Type.Array(moduleNamedSchema),
    queryCatalogEntries: Type.Array(moduleQueryCatalogSchema),
    exportHooks: Type.Array(moduleTransferHookSchema),
    importHooks: Type.Array(moduleTransferHookSchema),
    migrationNotes: stringArray,
    degradedReasons: stringArray,
    blockedReasons: stringArray,
  },
  { additionalProperties: false },
);

const installedModuleSchema = Type.Object(
  {
    moduleId: Type.String(),
    installedVersion: Type.Number(),
    descriptorFingerprint: Type.String(),
    installedAt: Type.String(),
    updatedAt: Type.String(),
  },
  { additionalProperties: false },
);

export const nativeRuntimeModuleSchemaRegistryDiagnosticsSchema = Type.Object(
  {
    source: Type.String(),
    backendCapabilities: stringArray,
    modules: Type.Array(moduleSchemaDiagnosticSchema),
    orphanInstalledModules: Type.Array(installedModuleSchema),
  },
  { additionalProperties: false },
);

const migrationSchema = Type.Object(
  {
    version: Type.Number(),
    description: Type.String(),
    appliedAt: Type.String(),
  },
  { additionalProperties: false },
);

const storageCapabilitySchema = Type.Object(
  {
    name: Type.String(),
    supported: Type.Boolean(),
    detail: Type.String(),
  },
  { additionalProperties: false },
);

const repositoryRequirementSchema = Type.Object(
  {
    capability: Type.String(),
    required: Type.Boolean(),
    detail: Type.String(),
  },
  { additionalProperties: false },
);

const repositoryGroupSchema = Type.Object(
  {
    groupId: Type.String(),
    label: Type.String(),
    correctnessSensitive: Type.Boolean(),
    backendRequirements: Type.Array(repositoryRequirementSchema),
    notes: stringArray,
  },
  { additionalProperties: false },
);

const connectionHealthSchema = Type.Object(
  {
    backend: Type.String(),
    status: Type.String(),
    maxConnections: Type.Number(),
    activeConnections: Type.Number(),
    idleConnections: Type.Number(),
    totalOpened: Type.Number(),
    checkoutCount: Type.Number(),
    checkoutReuseCount: Type.Number(),
    reconnectAttempts: Type.Number(),
    reconnectSuccesses: Type.Number(),
    closedConnectionsDiscarded: Type.Number(),
    lastError: optionalString,
  },
  { additionalProperties: false },
);

const queryPlanCheckSchema = Type.Object(
  {
    name: Type.String(),
    usesIndex: Type.Boolean(),
    detail: Type.String(),
  },
  { additionalProperties: false },
);

const pressureSignalSchema = Type.Object(
  {
    name: Type.String(),
    active: Type.Boolean(),
    severity: Type.String(),
    observedValue: Type.Number(),
    thresholdValue: Type.Optional(Type.Number()),
    detail: Type.String(),
  },
  { additionalProperties: false },
);

export const nativeRuntimeStorageDiagnosticsSchema = Type.Object(
  {
    backend: Type.String(),
    backendLabel: Type.String(),
    schemaVersion: Type.Number(),
    supportedSchemaVersion: Type.Number(),
    migrations: Type.Array(migrationSchema),
    size: nativeRuntimeDatabaseSizeSchema,
    tableCounts: Type.Array(
      Type.Object(
        { table: Type.String(), rows: Type.Number() },
        { additionalProperties: false },
      ),
    ),
    capabilities: Type.Array(storageCapabilitySchema),
    repositoryGroups: Type.Array(repositoryGroupSchema),
    connectionHealth: connectionHealthSchema,
    moduleRegistry: nativeRuntimeModuleSchemaRegistryDiagnosticsSchema,
    indexChecks: Type.Array(queryPlanCheckSchema),
    searchHealthy: Type.Boolean(),
    pressureSignals: Type.Array(pressureSignalSchema),
    pressure: Type.Boolean(),
  },
  { additionalProperties: false },
);

const sessionMemoryCompactionSchema = Type.Object(
  {
    enabled: Type.Boolean(),
    scopesInspected: Type.Number(),
    retentionPressureScopes: Type.Number(),
    scopesCompacted: Type.Number(),
    sessionSummariesCreated: Type.Number(),
    branchSummariesCreated: Type.Number(),
    recordsArchived: Type.Number(),
    recordsSuperseded: Type.Number(),
    skippedScopes: Type.Number(),
  },
  { additionalProperties: false },
);

export const nativeRuntimeMaintenanceReportSchema = Type.Object(
  {
    sizeBefore: nativeRuntimeDatabaseSizeSchema,
    sizeAfter: nativeRuntimeDatabaseSizeSchema,
    expiredQueueMessages: Type.Number(),
    purgedTerminalQueueMessages: Type.Number(),
    expiredProviderWireStates: Type.Number(),
    sessionMemoryCompaction: sessionMemoryCompactionSchema,
    walCheckpointRan: Type.Boolean(),
    optimizeRan: Type.Boolean(),
  },
  { additionalProperties: false },
);

export const nativeSimpleKvRecordSchema = Type.Object(
  {
    scopeType: Type.String(),
    scopeId: Type.String(),
    key: Type.String(),
    valueJson: Type.String(),
    revision: Type.Number(),
    createdAt: Type.String(),
    updatedAt: Type.String(),
    expiresAt: optionalString,
  },
  { additionalProperties: false },
);

export const nativeSimpleKvRecordArraySchema = Type.Array(
  nativeSimpleKvRecordSchema,
);
