import assert from "node:assert/strict";
import type { ProfileId } from "@rusty-crew/contracts";
import {
  createWebBrowserToolFinishedEvent,
  createWebBrowserToolStartedEvent,
  evaluateWebBrowserResourceHooks,
  webBrowserToolSource,
} from "./index.js";

assert.equal(webBrowserToolSource("web_extract"), "web");
assert.equal(webBrowserToolSource("browser_snapshot"), "browser");

const allowed = evaluateWebBrowserResourceHooks({
  toolName: "web_extract",
  toolProfile: { tools: [{ name: "web_extract" }] },
  profileId: "profile-web" as ProfileId,
  timeoutMs: 5_000,
});
assert.equal(allowed.allowed, true);
assert.equal(allowed.metadata.source, "web");
assert.equal(allowed.metadata.serverNames.length, 0);
assert.equal(allowed.metadata.profileId, "profile-web");
assert.equal(allowed.metadata.sourceToolName, "web_extract");
assert.equal(allowed.metadata.policy?.timeoutMs, 5_000);

const deniedByProfile = evaluateWebBrowserResourceHooks({
  toolName: "browser_snapshot",
  toolProfile: { tools: [{ name: "web_extract" }] },
});
assert.equal(deniedByProfile.allowed, false);
assert.equal(deniedByProfile.denialReason, "tool_profile_denied");
assert.equal(
  deniedByProfile.metadata.policy?.denialReason,
  "tool_profile_denied",
);
assert.equal(deniedByProfile.metadata.source, "browser");

const networkDenied = evaluateWebBrowserResourceHooks({
  toolName: "web_extract",
  resourceDeniedReason: "network_denied",
});
assert.equal(networkDenied.allowed, false);
assert.equal(networkDenied.metadata.policy?.denialReason, "network_denied");

const cancelled = evaluateWebBrowserResourceHooks({
  toolName: "browser_click",
  cancelled: true,
});
assert.equal(cancelled.allowed, false);
assert.equal(cancelled.metadata.policy?.cancelled, true);

const archived = evaluateWebBrowserResourceHooks({
  toolName: "browser_vision",
  sessionArchived: true,
});
assert.equal(archived.allowed, false);
assert.equal(archived.metadata.policy?.archiveCleanup, true);

const started = createWebBrowserToolStartedEvent({
  toolName: "browser_navigate",
  profileId: "profile-browser" as ProfileId,
});
assert.equal(started.type, "tool_call_started");
assert.equal(started.metadata?.source, "browser");
assert.equal(started.metadata?.policy?.allowed, true);

const finished = createWebBrowserToolFinishedEvent({
  toolName: "web_search",
  isError: true,
  allowed: false,
  denialReason: "timeout",
  timeoutMs: 1_000,
});
assert.equal(finished.type, "tool_call_finished");
assert.equal(finished.isError, true);
assert.equal(finished.metadata?.source, "web");
assert.equal(finished.metadata?.policy?.denialReason, "timeout");

const metadataText = JSON.stringify(finished.metadata);
assert.equal(metadataText.includes("page content"), false);
assert.equal(metadataText.includes("screenshot"), false);
assert.equal(metadataText.includes("base64"), false);

console.log(
  JSON.stringify(
    {
      allowed: allowed.allowed,
      denied: deniedByProfile.denialReason,
      networkDenied: networkDenied.denialReason,
      cancelled: cancelled.denialReason,
      archived: archived.denialReason,
      finishedSource: finished.metadata?.source,
    },
    null,
    2,
  ),
);
