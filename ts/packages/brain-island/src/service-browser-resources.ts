import type { SessionId } from "@rusty-crew/contracts";
import type {
  NativeBridgeModule,
  NativeWebBrowserResourcePolicyPlan,
} from "@rusty-crew/native-bridge";
import {
  BrowserSessionManager,
  type BrowserCleanupSummary,
  type BrowserCloseReason,
  type BrowserSessionLimits,
} from "./browser-session-manager.js";
import {
  MemoryBrowserScreenshotStore,
  type BrowserScreenshotStore,
} from "./browser-tools.js";

export interface ServiceBrowserResources {
  manager: BrowserSessionManager;
  screenshotStore: BrowserScreenshotStore;
  resourcePolicy: NativeWebBrowserResourcePolicyPlan;
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
    resourcePolicy?: NativeWebBrowserResourcePolicyPlan;
    bridge?: Pick<
      NativeBridgeModule,
      "beginRuntimeActivity" | "finishRuntimeActivity"
    >;
  } = {},
): ServiceBrowserResources {
  const resourcePolicy =
    input.resourcePolicy ?? defaultWebBrowserResourcePolicy;
  return {
    manager:
      input.manager ??
      new BrowserSessionManager({
        limits: browserSessionLimitsFromPolicy(resourcePolicy),
        activityBridge: input.bridge,
      }),
    screenshotStore:
      input.screenshotStore ?? new MemoryBrowserScreenshotStore(),
    resourcePolicy,
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

const defaultWebBrowserResourcePolicy: NativeWebBrowserResourcePolicyPlan = {
  web: {
    searchDefaultLimit: 5,
    searchMaxResults: 10,
    maxExtractUrls: 5,
    maxExtractChars: 24_000,
    maxExtractBytes: 512 * 1024,
    maxRedirects: 5,
    allowPrivateNet: false,
    allowedNonstandardPorts: [],
  },
  browser: {
    maxServiceSessions: 8,
    maxSessionsPerAgent: 2,
    idleTimeoutMs: 10 * 60 * 1000,
    hardLifetimeMs: 60 * 60 * 1000,
    startupTimeoutMs: 15_000,
    cdpCallTimeoutMs: 15_000,
    pageLoadTimeoutMs: 8_000,
    maxRefs: 80,
    consoleRingSize: 100,
    maxScreenshotBytes: 2 * 1024 * 1024,
    allowPrivateNet: false,
  },
  denialReasonCodes: [
    "invalid_url",
    "unsupported_scheme",
    "credentialed_url",
    "nonstandard_port",
    "private_network",
    "dns_resolution_failed",
    "too_many_redirects",
    "http_error",
    "unsupported_content_type",
    "fetch_failed",
    "browser_session_service_limit",
    "browser_session_agent_limit",
    "browser_session_profile_limit",
    "browser_session_not_ready",
    "browser_screenshot_too_large",
    "browser_screenshot_store_unavailable",
  ],
};

function browserSessionLimitsFromPolicy(
  policy: NativeWebBrowserResourcePolicyPlan,
): BrowserSessionLimits {
  const limits: BrowserSessionLimits = {
    maxServiceSessions: policy.browser.maxServiceSessions,
    maxSessionsPerAgent: policy.browser.maxSessionsPerAgent,
    idleTimeoutMs: policy.browser.idleTimeoutMs,
    hardLifetimeMs: policy.browser.hardLifetimeMs,
    startupTimeoutMs: policy.browser.startupTimeoutMs,
    cdpCallTimeoutMs: policy.browser.cdpCallTimeoutMs,
    maxRefs: policy.browser.maxRefs,
    consoleRingSize: policy.browser.consoleRingSize,
  };
  if (policy.browser.maxSessionsPerProfile !== undefined) {
    limits.maxSessionsPerProfile = policy.browser.maxSessionsPerProfile;
  }
  return limits;
}
