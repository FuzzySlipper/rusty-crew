import { validateBridgeValue } from "./bridge-validation.js";
import type { NativeBridgeBinding } from "./generated/native-binding-surface.js";
import type {
  NativeBridgeModule,
  NativeRuntimeDatabaseSize,
  NativeRuntimeMaintenanceReport,
  NativeRuntimeModuleSchemaRegistryDiagnostics,
  NativeRuntimeStorageDiagnostics,
  NativeSimpleKvRecord,
} from "./public-api.js";
import {
  nativeRuntimeDatabaseSizeSchema,
  nativeRuntimeMaintenanceReportSchema,
  nativeRuntimeModuleSchemaRegistryDiagnosticsSchema,
  nativeRuntimeStorageDiagnosticsSchema,
  nativeSimpleKvRecordArraySchema,
  nativeSimpleKvRecordSchema,
} from "./native-admin-validation-schemas.js";

type AdminMethodName =
  | "diagnosticCountRows"
  | "databaseSize"
  | "storageDiagnostics"
  | "storageSchema"
  | "runMaintenance"
  | "listSimpleKv"
  | "putSimpleKv"
  | "deleteSimpleKv"
  | "searchRuntime"
  | "queryRuntimeCounters"
  | "runtimeSummary"
  | "resetRuntimeCounters";

export function createNativeBridgeAdminMethods(
  binding: NativeBridgeBinding,
): Pick<NativeBridgeModule, AdminMethodName> {
  return {
    diagnosticCountRows: async (table) => binding.countRows(table),
    databaseSize: async () =>
      validateBridgeValue<NativeRuntimeDatabaseSize>({
        operation: "database_size",
        direction: "rust_to_ts",
        schema: nativeRuntimeDatabaseSizeSchema,
        value: binding.databaseSize(),
      }),
    storageDiagnostics: async () =>
      validateBridgeValue<NativeRuntimeStorageDiagnostics>({
        operation: "storage_diagnostics",
        direction: "rust_to_ts",
        schema: nativeRuntimeStorageDiagnosticsSchema,
        value: binding.storageDiagnostics(),
      }),
    storageSchema: async () =>
      validateBridgeValue<NativeRuntimeModuleSchemaRegistryDiagnostics>({
        operation: "storage_schema",
        direction: "rust_to_ts",
        schema: nativeRuntimeModuleSchemaRegistryDiagnosticsSchema,
        value: binding.storageSchema(),
      }),
    runMaintenance: async (policy) =>
      validateBridgeValue<NativeRuntimeMaintenanceReport>({
        operation: "run_maintenance",
        direction: "rust_to_ts",
        schema: nativeRuntimeMaintenanceReportSchema,
        value: binding.runMaintenance(policy),
      }),
    listSimpleKv: async (query) =>
      validateBridgeValue<NativeSimpleKvRecord[]>({
        operation: "list_simple_kv",
        direction: "rust_to_ts",
        schema: nativeSimpleKvRecordArraySchema,
        value: binding.listSimpleKv(query),
      }),
    putSimpleKv: async (write) =>
      validateBridgeValue<NativeSimpleKvRecord>({
        operation: "put_simple_kv",
        direction: "rust_to_ts",
        schema: nativeSimpleKvRecordSchema,
        value: binding.putSimpleKv(write),
      }),
    deleteSimpleKv: async (input) =>
      validateBridgeValue<NativeSimpleKvRecord>({
        operation: "delete_simple_kv",
        direction: "rust_to_ts",
        schema: nativeSimpleKvRecordSchema,
        value: binding.deleteSimpleKv(input),
      }),
    searchRuntime: async (query) =>
      binding.searchRuntime(query) as unknown as Awaited<
        ReturnType<NativeBridgeModule["searchRuntime"]>
      >,
    queryRuntimeCounters: async (query) =>
      binding.queryRuntimeCounters(query) as unknown as Awaited<
        ReturnType<NativeBridgeModule["queryRuntimeCounters"]>
      >,
    runtimeSummary: async (input) =>
      binding.runtimeSummary(
        input.scopeType,
        input.scopeId,
      ) as unknown as Awaited<ReturnType<NativeBridgeModule["runtimeSummary"]>>,
    resetRuntimeCounters: async (query) => binding.resetRuntimeCounters(query),
  };
}
