import type {
  AdapterId,
  PlatformAdapterRegistration,
} from "@rusty-crew/contracts";

export function createTuiAdapterRegistration(
  adapterId: AdapterId,
): PlatformAdapterRegistration {
  return { adapterId, kind: "tui", displayName: "TUI" };
}

export {
  createDebugTuiState,
  loadDebugTuiState,
  reduceDebugTuiState,
  renderDebugTui,
} from "./debug-tui.js";
export type {
  DebugTuiApiClient,
  DebugTuiChannelBinding,
  DebugTuiDiagnosticsBundle,
  DebugTuiDiagnosticsOverview,
  DebugTuiDirectDebugContext,
  DebugTuiHealthProjection,
  DebugTuiKey,
  DebugTuiLoadOptions,
  DebugTuiMcpSurface,
  DebugTuiMetric,
  DebugTuiObservation,
  DebugTuiPage,
  DebugTuiQuery,
  DebugTuiRecentEvent,
  DebugTuiRenderOptions,
  DebugTuiSession,
  DebugTuiSnapshot,
  DebugTuiState,
  DebugTuiTab,
  DebugTuiToolCatalog,
} from "./debug-tui.js";
