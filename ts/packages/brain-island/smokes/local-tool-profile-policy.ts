import assert from "node:assert/strict";
import { loadNativeBridge } from "@rusty-crew/native-bridge";
import type {
  NativeBridgeModule,
  NativeLocalToolProfilePolicyValidationInput,
  NativeLocalToolProfilePolicyValidationResult,
  NativeSimpleKvDelete,
  NativeSimpleKvQuery,
  NativeSimpleKvRecord,
  NativeSimpleKvWrite,
} from "@rusty-crew/native-bridge";
import {
  createLocalToolProfileStore,
  LocalToolProfileError,
} from "../src/local-tool-profiles.js";

const nativeBridge = await loadNativeBridge();
const bridge = inMemoryLocalToolProfileBridge(nativeBridge);
const store = createLocalToolProfileStore({
  bridge,
  now: fixedNow,
});

const seededProfiles = await store.list();
assert.ok(
  seededProfiles.items.some((profile) => profile.id === "full_agent"),
  "seeded system profiles must validate through the native policy path",
);
assert.ok(
  bridge.validations.some(
    (input) => input.profile.id === "full_agent" && input.profile.system,
  ),
  "native policy validator was not used while seeding system profiles",
);

await assert.rejects(
  () =>
    store.create({
      id: "bad_toolset",
      displayName: "Bad Toolset",
      toolsets: ["missing_toolset"],
    }),
  (error) =>
    error instanceof LocalToolProfileError &&
    error.reasonCode === "local_tool_profile_unknown_toolset",
);

await assert.rejects(
  () =>
    store.create({
      id: "bad_tool",
      displayName: "Bad Tool",
      tools: ["missing_tool"],
    }),
  (error) =>
    error instanceof LocalToolProfileError &&
    error.reasonCode === "local_tool_profile_unknown_tool",
);

await assert.rejects(
  () =>
    store.create({
      id: "bad_mcp_toolset",
      displayName: "Bad MCP Toolset",
      toolsets: ["mcp:den"],
    }),
  (error) =>
    error instanceof LocalToolProfileError &&
    error.reasonCode === "local_tool_profile_rejects_mcp_toolset",
);

const customProfile = await store.create({
  id: "custom_code_reader",
  displayName: "Custom Code Reader",
  toolsets: ["local_code_read"],
  tools: ["todo"],
});
assert.deepEqual(customProfile.toolsets, ["local_code_read"]);
assert.deepEqual(customProfile.tools, ["todo"]);

const resolvedProfile = await store.resolve("custom_code_reader");
assert.deepEqual(resolvedProfile.toolPolicy.requestedToolsets, [
  "local_code_read",
]);
assert.deepEqual(resolvedProfile.toolPolicy.requestedTools, ["todo"]);
assert.ok(
  bridge.validations.some(
    (input) =>
      input.profile.id === "bad_mcp_toolset" &&
      input.profile.toolsets.includes("mcp:den"),
  ),
  "dynamic MCP local-profile denial must come from the native policy path",
);

console.log(
  JSON.stringify(
    {
      seededProfiles: seededProfiles.total,
      validationCalls: bridge.validations.length,
      createdProfile: customProfile.id,
    },
    null,
    2,
  ),
);

function inMemoryLocalToolProfileBridge(
  nativeBridge: Pick<NativeBridgeModule, "validateLocalToolProfilePolicy">,
): Pick<
  NativeBridgeModule,
  | "listSimpleKv"
  | "putSimpleKv"
  | "deleteSimpleKv"
  | "validateLocalToolProfilePolicy"
> & {
  validations: NativeLocalToolProfilePolicyValidationInput[];
} {
  const records = new Map<string, NativeSimpleKvRecord>();
  const validations: NativeLocalToolProfilePolicyValidationInput[] = [];
  let revision = 0;
  return {
    validations,
    async validateLocalToolProfilePolicy(
      input,
    ): Promise<NativeLocalToolProfilePolicyValidationResult> {
      validations.push(input);
      return nativeBridge.validateLocalToolProfilePolicy(input);
    },
    async listSimpleKv(query: NativeSimpleKvQuery) {
      return [...records.values()]
        .filter(
          (record) =>
            record.scopeType === query.scopeType &&
            record.scopeId === query.scopeId &&
            (query.keyPrefix === undefined ||
              record.key.startsWith(query.keyPrefix)),
        )
        .sort((left, right) => left.key.localeCompare(right.key))
        .slice(0, query.limit ?? 1_000);
    },
    async putSimpleKv(write: NativeSimpleKvWrite) {
      const existing = records.get(write.key);
      const record: NativeSimpleKvRecord = {
        scopeType: write.scopeType,
        scopeId: write.scopeId,
        key: write.key,
        valueJson: write.valueJson,
        revision: ++revision,
        createdAt: existing?.createdAt ?? write.now,
        updatedAt: write.now,
        expiresAt: write.expiresAt,
      };
      records.set(write.key, record);
      return record;
    },
    async deleteSimpleKv(input: NativeSimpleKvDelete) {
      const existing = records.get(input.key);
      assert.ok(existing, `missing simple kv record ${input.key}`);
      assert.equal(existing.revision, input.expectedRevision);
      records.delete(input.key);
      return existing;
    },
  };
}

function fixedNow(): string {
  return "2026-07-08T00:00:00.000Z";
}
