import type {
  BrowserCleanupSummary,
  BrowserManagerDiagnostics,
  BrowserSessionManager,
} from "./browser-session-manager.js";
import type { ToolInventoryRequest } from "./tool-registry.js";

export interface WebDiagnosticsInput {
  env?: Readonly<Record<string, string | undefined>>;
  searxngUrl?: string;
  networkEnabled?: boolean;
  allowPrivateNet?: boolean;
}

export interface BrowserDiagnosticsInput {
  manager?: BrowserSessionManager;
  browserBinaryPath?: string;
  browserBinaryAvailable?: boolean;
  screenshotStoreConfigured?: boolean;
}

export interface WebBrowserDiagnosticsInput
  extends WebDiagnosticsInput, BrowserDiagnosticsInput {}

export interface WebProviderDiagnostics {
  provider: "searxng" | "duckduckgo_html";
  networkEnabled: boolean;
  searxngConfigured: boolean;
  searxngHost?: string;
  allowPrivateNet: boolean;
}

export interface BrowserCapabilityDiagnostics {
  binaryPathLabel: string;
  binaryAvailability: "available" | "unavailable" | "unknown";
  screenshotStoreConfigured: boolean;
  manager?: BrowserManagerDiagnostics;
}

export interface WebBrowserDiagnostics {
  web: WebProviderDiagnostics;
  browser: BrowserCapabilityDiagnostics;
  inventoryRequest: ToolInventoryRequest;
}

export function buildWebBrowserDiagnostics(
  input: WebBrowserDiagnosticsInput = {},
): WebBrowserDiagnostics {
  const web = webProviderDiagnostics(input);
  const browser = browserDiagnostics(input);
  return {
    web,
    browser,
    inventoryRequest: webBrowserInventoryRequest(web, browser),
  };
}

export async function cleanupWebBrowserCapabilities(input: {
  manager: BrowserSessionManager;
  now?: Date;
}): Promise<BrowserCleanupSummary> {
  return input.manager.sweep(input.now);
}

function webProviderDiagnostics(
  input: WebDiagnosticsInput,
): WebProviderDiagnostics {
  const env = input.env ?? process.env;
  const searxngUrl =
    input.searxngUrl ?? env.RUSTY_CREW_SEARXNG_URL ?? env.PI_CREW_SEARXNG_URL;
  return {
    provider: searxngUrl ? "searxng" : "duckduckgo_html",
    networkEnabled: input.networkEnabled ?? true,
    searxngConfigured: Boolean(searxngUrl),
    searxngHost: searxngUrl ? safeHost(searxngUrl) : undefined,
    allowPrivateNet:
      input.allowPrivateNet ??
      (env.RUSTY_CREW_ALLOW_PRIVATE_NET === "1" ||
        env.PI_CREW_ALLOW_PRIVATE_NET === "1"),
  };
}

function browserDiagnostics(
  input: BrowserDiagnosticsInput,
): BrowserCapabilityDiagnostics {
  const binaryPath =
    input.browserBinaryPath ?? process.env.RUSTY_CREW_CHROMIUM_PATH;
  const binaryPathLabel = binaryPath ? redactPath(binaryPath) : "chromium";
  return {
    binaryPathLabel,
    binaryAvailability:
      input.browserBinaryAvailable === undefined
        ? "unknown"
        : input.browserBinaryAvailable
          ? "available"
          : "unavailable",
    screenshotStoreConfigured: input.screenshotStoreConfigured ?? false,
    manager: input.manager?.diagnostics(),
  };
}

function webBrowserInventoryRequest(
  web: WebProviderDiagnostics,
  browser: BrowserCapabilityDiagnostics,
): ToolInventoryRequest {
  const resourceDeniedTools: string[] = [];
  const resourceDeniedReasons: Record<string, string> = {};
  if (!web.networkEnabled) {
    deny("web_search", "network access is disabled");
    deny("web_extract", "network access is disabled");
  }
  if (browser.binaryAvailability === "unavailable") {
    for (const tool of [
      "browser_navigate",
      "browser_snapshot",
      "browser_click",
      "browser_type",
      "browser_scroll",
      "browser_back",
      "browser_press",
      "browser_console",
      "browser_vision",
    ]) {
      deny(tool, "browser binary is unavailable");
    }
  }
  if (!browser.screenshotStoreConfigured) {
    deny(
      "browser_vision",
      "browser screenshot artifact store is not configured",
    );
  }
  return {
    resourceDeniedTools,
    resourceDeniedReasons,
  };

  function deny(toolName: string, reason: string): void {
    if (!resourceDeniedTools.includes(toolName)) {
      resourceDeniedTools.push(toolName);
    }
    resourceDeniedReasons[toolName] = reason;
  }
}

function safeHost(rawUrl: string): string | undefined {
  try {
    return new URL(rawUrl).host;
  } catch {
    return "invalid-url";
  }
}

function redactPath(path: string): string {
  const parts = path.split("/");
  return parts.at(-1) || path;
}
