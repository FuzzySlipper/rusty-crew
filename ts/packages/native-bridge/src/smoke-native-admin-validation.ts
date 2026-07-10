import assert from "node:assert/strict";

import { Value } from "typebox/value";

import {
  nativeRuntimeDatabaseSizeSchema,
  nativeRuntimeMaintenanceReportSchema,
  nativeRuntimeModuleSchemaRegistryDiagnosticsSchema,
  nativeRuntimeStorageDiagnosticsSchema,
  nativeSimpleKvRecordArraySchema,
} from "./native-admin-validation-schemas.js";

const size = {
  databaseBytes: 1024,
  pageCount: 4,
  pageSizeBytes: 4096,
  freelistPages: 0,
  freelistBytes: 0,
  walBytes: 0,
};
const moduleRegistry = {
  source: "sqlite",
  backendCapabilities: ["transactions"],
  modules: [],
  orphanInstalledModules: [],
};
const maintenance = {
  sizeBefore: size,
  sizeAfter: size,
  expiredQueueMessages: 0,
  purgedTerminalQueueMessages: 0,
  expiredProviderWireStates: 0,
  sessionMemoryCompaction: {
    enabled: true,
    scopesInspected: 1,
    retentionPressureScopes: 0,
    scopesCompacted: 0,
    sessionSummariesCreated: 0,
    branchSummariesCreated: 0,
    recordsArchived: 0,
    recordsSuperseded: 0,
    skippedScopes: 0,
  },
  walCheckpointRan: false,
  optimizeRan: false,
};
const diagnostics = {
  backend: "sqlite",
  backendLabel: "SQLite",
  schemaVersion: 1,
  supportedSchemaVersion: 1,
  migrations: [],
  size,
  tableCounts: [],
  capabilities: [],
  repositoryGroups: [],
  connectionHealth: {
    backend: "sqlite",
    status: "ready",
    maxConnections: 1,
    activeConnections: 1,
    idleConnections: 0,
    totalOpened: 1,
    checkoutCount: 1,
    checkoutReuseCount: 0,
    reconnectAttempts: 0,
    reconnectSuccesses: 0,
    closedConnectionsDiscarded: 0,
  },
  moduleRegistry,
  indexChecks: [],
  searchHealthy: true,
  pressureSignals: [],
  pressure: false,
};
const simpleKv = [
  {
    scopeType: "runtime",
    scopeId: "_global",
    key: "test",
    valueJson: "{}",
    revision: 1,
    createdAt: "2026-07-10T00:00:00Z",
    updatedAt: "2026-07-10T00:00:00Z",
  },
];

assert(Value.Check(nativeRuntimeDatabaseSizeSchema, size));
assert(
  Value.Check(
    nativeRuntimeModuleSchemaRegistryDiagnosticsSchema,
    moduleRegistry,
  ),
);
assert(Value.Check(nativeRuntimeMaintenanceReportSchema, maintenance));
assert(Value.Check(nativeRuntimeStorageDiagnosticsSchema, diagnostics));
assert(Value.Check(nativeSimpleKvRecordArraySchema, simpleKv));

assert(
  !Value.Check(nativeRuntimeDatabaseSizeSchema, { ...size, surprise: true }),
  "native admin schemas must reject untracked napi output fields",
);
const invalidDiagnostics = structuredClone(diagnostics) as {
  connectionHealth: { status?: string };
};
delete invalidDiagnostics.connectionHealth.status;
assert(
  !Value.Check(nativeRuntimeStorageDiagnosticsSchema, invalidDiagnostics),
  "native admin schemas must reject missing nested diagnostics fields",
);

console.log(
  JSON.stringify({
    nativeAdminSchemas: 5,
    strictAdditionalProperties: true,
    nestedDiagnosticsCoverage: true,
  }),
);
