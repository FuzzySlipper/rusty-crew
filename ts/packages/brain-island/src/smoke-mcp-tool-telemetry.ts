import assert from "node:assert/strict";
import { convertMcpToolsToCandidates } from "@rusty-crew/adapter-mcp";
import type {
  AdapterId,
  AgentId,
  McpBindingRecord,
  ProfileId,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createMcpToolFinishedEvent,
  createMcpToolStartedEvent,
  evaluateMcpResourceHooks,
} from "./index.js";

const binding: McpBindingRecord = {
  bindingId: "mcp-alpha",
  adapterId: "mcp-ts-main" as AdapterId,
  agentId: "agent-alpha" as AgentId,
  sessionId: "session-alpha" as SessionId,
  profileId: "prime-profile" as ProfileId,
  serverNames: ["den"],
  endpointRef: "config://mcp/alpha",
  transport: "stdio",
  toolProfileKey: "prime-mcp",
  discoveredToolRevision: "rev-alpha",
  status: "active",
  diagnostics: {},
};

const [candidate] = convertMcpToolsToCandidates(binding, [
  {
    name: "search",
    description: "Search Den memory.",
    inputSchema: {
      type: "object",
      properties: { query: { type: "string", minLength: 1 } },
      required: ["query"],
    },
  },
]).candidates;

assert.ok(candidate);

const allowed = evaluateMcpResourceHooks({
  binding,
  candidate,
  toolProfile: { tools: [{ name: candidate.name }] },
  timeoutMs: 5_000,
});
assert.equal(allowed.allowed, true);
assert.equal(allowed.metadata.source, "mcp");
assert.equal(allowed.metadata.bindingId, "mcp-alpha");
assert.equal(allowed.metadata.sourceToolName, "search");
assert.equal(allowed.metadata.policy?.timeoutMs, 5_000);

const denied = evaluateMcpResourceHooks({
  binding,
  candidate,
  toolProfile: { tools: [{ name: "other_tool" }] },
});
assert.equal(denied.allowed, false);
assert.equal(denied.denialReason, "tool_profile_denied");
assert.equal(denied.metadata.policy?.denialReason, "tool_profile_denied");

const cancelled = evaluateMcpResourceHooks({
  binding,
  candidate,
  cancelled: true,
});
assert.equal(cancelled.allowed, false);
assert.equal(cancelled.denialReason, "cancelled");
assert.equal(cancelled.metadata.policy?.cancelled, true);

const archived = evaluateMcpResourceHooks({
  binding,
  candidate,
  sessionArchived: true,
});
assert.equal(archived.allowed, false);
assert.equal(archived.denialReason, "session_archived");
assert.equal(archived.metadata.policy?.archiveCleanup, true);

const started = createMcpToolStartedEvent({
  binding,
  toolName: candidate.name,
  sourceToolName: candidate.source.sourceToolName,
});
assert.equal(started.type, "tool_call_started");
assert.equal(started.metadata?.policy?.allowed, true);

const finished = createMcpToolFinishedEvent({
  binding,
  toolName: candidate.name,
  sourceToolName: candidate.source.sourceToolName,
  isError: true,
  allowed: true,
  timeoutMs: 5_000,
});
assert.equal(finished.type, "tool_call_finished");
assert.equal(finished.isError, true);
assert.equal(finished.metadata?.policy?.timeoutMs, 5_000);

console.log(
  JSON.stringify(
    {
      allowed: allowed.allowed,
      denied: denied.denialReason,
      cancelled: cancelled.denialReason,
      archived: archived.denialReason,
      eventMetadataSource: finished.metadata?.source,
    },
    null,
    2,
  ),
);
