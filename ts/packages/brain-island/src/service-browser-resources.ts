import type { SessionId } from "@rusty-crew/contracts";
import {
  BrowserSessionManager,
  type BrowserCleanupSummary,
  type BrowserCloseReason,
} from "./browser-session-manager.js";
import {
  MemoryBrowserScreenshotStore,
  type BrowserScreenshotStore,
} from "./browser-tools.js";

export interface ServiceBrowserResources {
  manager: BrowserSessionManager;
  screenshotStore: BrowserScreenshotStore;
}

export interface ServiceBrowserSessionCleanup {
  sessionId: SessionId;
  reason: BrowserCloseReason;
  closed: boolean;
}

export function createServiceBrowserResources(
  input: {
    manager?: BrowserSessionManager;
    screenshotStore?: BrowserScreenshotStore;
  } = {},
): ServiceBrowserResources {
  return {
    manager: input.manager ?? new BrowserSessionManager(),
    screenshotStore:
      input.screenshotStore ?? new MemoryBrowserScreenshotStore(),
  };
}

export async function closeServiceBrowserSessionForLifecycle(input: {
  resources: ServiceBrowserResources;
  sessionId: SessionId;
  reason: BrowserCloseReason;
}): Promise<ServiceBrowserSessionCleanup> {
  const existed = input.resources.manager
    .diagnostics()
    .sessions.some((session) => session.sessionId === input.sessionId);
  await input.resources.manager.close(input.sessionId, input.reason);
  return {
    sessionId: input.sessionId,
    reason: input.reason,
    closed: existed,
  };
}

export async function closeAllServiceBrowserSessionsForLifecycle(input: {
  resources: ServiceBrowserResources;
  reason: BrowserCloseReason;
}): Promise<BrowserCleanupSummary> {
  return input.resources.manager.closeAll(input.reason);
}
