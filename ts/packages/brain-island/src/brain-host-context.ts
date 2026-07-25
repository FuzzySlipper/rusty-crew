import type { BrainProviderStateScope } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { BrainActionPlanner } from "./index.js";
import type { LoadedProfileContext } from "./profile-loading.js";
import type { ProviderRequestDebugStore } from "./provider-request-debug-store.js";
import type { RustyCrewServiceConfig } from "./service-config.js";
import type { RustyCrewRuntimeConfig } from "./service-runtime-config.js";
import type { ToolCallDebugStore } from "./tool-call-debug-store.js";
import type { BrainToolResolver } from "./tool-session-selection.js";

export interface BrainHostContext {
  profile: LoadedProfileContext;
  serviceConfig?: RustyCrewServiceConfig;
  runtimeConfig?: RustyCrewRuntimeConfig;
  bridge?: NativeBridgeModule;
  providerStateScope?: BrainProviderStateScope;
  toolResolver?: BrainToolResolver;
  prepareToolResolution?: () => Promise<void>;
  planActions?: BrainActionPlanner;
  maxTokens?: number;
  toolCallDebugStore?: ToolCallDebugStore;
  providerRequestDebugStore?: ProviderRequestDebugStore;
}
