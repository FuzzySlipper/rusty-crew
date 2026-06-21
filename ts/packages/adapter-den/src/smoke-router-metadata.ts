import assert from "node:assert/strict";
import type {
  AdapterId,
  AgentId,
  ProfileId,
  ResultReference,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createDenRouterMetadataProjection,
  denProductWorkRef,
} from "./index.js";

const adapterId = "den-adapter" as AdapterId;
const agentId = "agent-alpha" as AgentId;
const sessionId = "session-alpha" as SessionId;
const profileId = "profile-alpha" as ProfileId;

const workRef = denProductWorkRef({
  refKind: "assignment",
  id: "assignment-123",
  projectId: "rusty-crew",
  label: "Implement projection contracts",
});
assert.equal(workRef.kind, "work_ref.v1");
assert.equal(workRef.sourceDomain, "den");
assert.equal(workRef.refKind, "assignment");

const completionRef: ResultReference = {
  kind: "result_ref.v1",
  sourceDomain: "runtime",
  refKind: "completion_packet",
  id: "session-alpha:42",
  label: "completion packet 42",
};

const projection = createDenRouterMetadataProjection({
  adapterId,
  bindingId: "binding-alpha",
  runtime: { agentId, sessionId, profileId },
  providerRefs: {
    provider: "den_channels",
    externalChannelId: "channel-1",
    externalThreadId: "thread-1",
  },
  denWorkRefs: [
    {
      refKind: "task",
      id: "3052",
      projectId: "rusty-crew",
      label: "Define Den work refs",
    },
    {
      refKind: "assignment",
      id: "assignment-123",
      projectId: "rusty-crew",
    },
  ],
  resultRefs: [completionRef],
  toolProfileKey: "prime-default",
  mcpSurfaceRefs: ["mcp:den:default"],
  status: "active",
  observedAt: "2026-06-20T08:00:00Z",
  provenance: {
    source: "smoke",
    providerToken: "should-not-leak",
    rawPrompt: "should-not-leak",
    rawToolOutput: "should-not-leak",
  },
});

assert.equal(projection.kind, "den_router_metadata_projection.v1");
assert.equal(projection.runtime.agentId, agentId);
assert.equal(projection.workRefs.length, 2);
assert.equal(projection.resultRefs?.[0]?.refKind, "completion_packet");
assert.equal(projection.provenance.providerToken, "[redacted]");
assert.equal(projection.provenance.rawPrompt, "[redacted]");
assert.equal(projection.provenance.rawToolOutput, "[redacted]");
assert.equal(projection.provenance.source, "smoke");

console.log(
  JSON.stringify(
    {
      bindingId: projection.bindingId,
      workRefs: projection.workRefs.map(
        (ref) => `${ref.sourceDomain}:${ref.refKind}:${ref.id}`,
      ),
      resultRefs: projection.resultRefs?.map(
        (ref) => `${ref.sourceDomain}:${ref.refKind}:${ref.id}`,
      ),
      redacted: [
        projection.provenance.providerToken,
        projection.provenance.rawPrompt,
        projection.provenance.rawToolOutput,
      ],
    },
    null,
    2,
  ),
);
