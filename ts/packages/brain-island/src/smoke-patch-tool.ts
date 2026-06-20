import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type {
  AgentEvent as PiAgentEvent,
  AgentMessage as PiAgentMessage,
  AgentOptions as PiAgentOptions,
  AgentToolResult,
} from "@earendil-works/pi-agent-core";
import type {
  AgentId,
  ProfileId,
  SessionHandle,
  SessionId,
} from "@rusty-crew/contracts";
import {
  createPiAgentBrain,
  defaultBodyDeltaPolicy,
  patchTool,
  resolveLocalCodeTools,
  selectToolProfile,
} from "./index.js";

const workdir = mkdtempSync(join(tmpdir(), "rusty-crew-patch-tool-"));
writeFileSync(join(workdir, "target.txt"), "hello world\n", "utf8");

const sessionId = "patch-tool-session" as SessionId;
const agentId = "patch-tool-agent" as AgentId;
const profileId = "patch-tool-profile" as ProfileId;
const selection = selectToolProfile({
  profileId,
  policy: {
    requestedTools: ["patch"],
  },
});

class PatchCallingFakeAgent {
  constructor(
    private readonly options: PiAgentOptions,
    private readonly results: Record<string, AgentToolResult<unknown>>,
  ) {}

  subscribe(
    _listener: (event: PiAgentEvent, signal: AbortSignal) => void,
  ): () => void {
    return () => {};
  }

  async prompt(
    _input: PiAgentMessage | PiAgentMessage[] | string,
  ): Promise<void> {
    const patch = this.options.initialState?.tools?.find(
      (tool) => tool.name === "patch",
    );
    assert.ok(patch);
    this.results.patch = await patch.execute("patch-call", {
      path: "target.txt",
      old_string: "hello world",
      new_string: "hello patch",
    });
  }

  async waitForIdle(): Promise<void> {}

  clearAllQueues(): void {}
}

const results: Record<string, AgentToolResult<unknown>> = {};
const brain = createPiAgentBrain({
  createAgent: (options) => new PatchCallingFakeAgent(options, results),
  resolveTools: resolveLocalCodeTools,
});

try {
  await brain.wake({
    wakeId: "wake-patch-tool",
    sessionId,
    systemPrompt: "system",
    roleAssembly: { instructions: "invoke patch" },
    state: {
      session: {
        handle: 1 as SessionHandle,
        sessionId,
        agentId,
        profileId,
        kind: "full",
        resourceLimits: {
          workdir,
          maxDurationMs: 5_000,
        },
        toolProfile: selection.toolProfile,
        status: "idle",
        brainTurnCount: 0,
        createdAt: "2026-06-20T00:00:00Z",
        lastActiveAt: "2026-06-20T00:00:00Z",
      },
      pendingMessages: [],
      recentEvents: [],
      childCompletions: [],
      fanOutGroups: [],
      deltaPolicy: defaultBodyDeltaPolicy,
    },
  });

  const patchResult = results.patch;
  assert.ok(patchResult);
  assert.equal(
    (patchResult.details as { ok: boolean }).ok,
    true,
    textResult(patchResult),
  );
  assert.equal(
    readFileSync(join(workdir, "target.txt"), "utf8"),
    "hello patch\n",
  );
  assert.match(textResult(patchResult), /--- target\.txt/);
  assert.match(textResult(patchResult), /\+hello patch/);

  writeFileSync(join(workdir, "a.txt"), "line1\nold_line\nline3\n", "utf8");
  const directPatch = patchTool({ workdir, maxDurationMs: 5_000 });
  const v4aResult = await directPatch.execute("patch-v4a", {
    mode: "patch",
    patch: [
      "*** Begin Patch",
      "*** Update File: a.txt",
      "@@ a-block @@",
      "-old_line",
      "+new_line",
      "*** End Patch",
    ].join("\n"),
  });
  assert.equal((v4aResult.details as { ok: boolean }).ok, true);
  assert.equal(
    readFileSync(join(workdir, "a.txt"), "utf8"),
    "line1\nnew_line\nline3\n",
  );

  writeFileSync(join(workdir, "config.json"), '{"ok": true}\n', "utf8");
  const rollbackResult = await directPatch.execute("patch-json-rollback", {
    path: "config.json",
    old_string: "true",
    new_string: "{",
  });
  assert.equal((rollbackResult.details as { ok: boolean }).ok, false);
  assert.match(textResult(rollbackResult), /rolled back/);
  assert.equal(
    readFileSync(join(workdir, "config.json"), "utf8"),
    '{"ok": true}\n',
  );

  console.log(
    JSON.stringify(
      {
        selectedTools: selection.toolProfile.tools.map((tool) => tool.name),
        patchedText: readFileSync(join(workdir, "target.txt"), "utf8").trim(),
        v4aPatchedText: readFileSync(join(workdir, "a.txt"), "utf8").trim(),
        rollbackPreservedJson: readFileSync(
          join(workdir, "config.json"),
          "utf8",
        ).trim(),
        diffLines: textResult(patchResult).split("\n").length,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(workdir, { force: true, recursive: true });
}

function textResult(result: AgentToolResult<unknown>): string {
  return result.content
    .flatMap((content) =>
      content.type === "text" && typeof content.text === "string"
        ? [content.text]
        : [],
    )
    .join("");
}
