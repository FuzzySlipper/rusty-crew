import assert from "node:assert/strict";
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { NativeSessionMemoryPromptContext } from "@rusty-crew/native-bridge";
import {
  buildProfileRoleAssembly,
  loadProfileContext,
  renderSessionMemoryContext,
} from "./index.js";
import type { ProfileId } from "@rusty-crew/contracts";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-session-memory-prompt-"));
const profilesDir = join(root, "profiles");
const skillsDir = join(root, "skills");
mkdirSync(profilesDir, { recursive: true });
mkdirSync(skillsDir, { recursive: true });

try {
  writeFileSync(
    join(profilesDir, "runner.json"),
    JSON.stringify(
      {
        profileId: "runner",
        displayName: "Runner",
        modelConfig: {
          provider: "local",
          modelName: "deterministic",
        },
        runtime: {
          defaultResourceLimits: {
            workdir: "/home/dev/rusty-crew",
          },
        },
        memoryConfig: {
          enabled: true,
          sessionMemory: true,
          sessionMemoryPrompt: {
            enabled: true,
            maxRecords: 2,
            includeAncestors: true,
            includeSiblings: false,
          },
        },
        prompt: {
          instructions: ["Use durable session memory carefully."],
        },
      },
      null,
      2,
    ),
  );

  const profileContext = await loadProfileContext({
    profilesDir,
    skillsDir,
    profileId: "runner" as ProfileId,
  });
  assert.equal(profileContext.profile.memoryConfig?.sessionMemory, true);
  assert.equal(
    profileContext.profile.memoryConfig?.sessionMemoryPrompt?.includeSiblings,
    false,
  );

  const context: NativeSessionMemoryPromptContext = {
    records: [
      sessionMemoryRecord({
        record_id: "memory-active-branch",
        shape_id: "branch_summary",
        scope_type: "branch",
        scope_id: "branch-active",
        branch_id: "branch-active",
        content: {
          summary: "Active branch is investigating the SQLite/Postgres seam.",
        },
      }),
      sessionMemoryRecord({
        record_id: "memory-session-choice",
        shape_id: "user_choice",
        scope_type: "session",
        scope_id: "session-alpha",
        branch_id: null,
        content: {
          choice: "Keep sibling branch lore out of default prompt context.",
        },
      }),
    ],
    diagnostics: {
      descriptor_id: "session_memory",
      descriptor_schema_version: 1,
      session_id: "session-alpha",
      active_branch_id: "branch-active",
      selected_records: [
        { record_id: "memory-active-branch", shape_id: "branch_summary" },
        { record_id: "memory-session-choice", shape_id: "user_choice" },
      ],
      excluded_counts: {
        wrong_branch: 1,
        sibling_branch: 1,
        tool_only: 1,
        archived: 1,
        superseded: 1,
        limit_exceeded: 1,
        policy_disabled: 1,
      },
      character_estimate: 121,
      token_estimate: 31,
      context_policy: "summary_context",
    },
  };
  const section = renderSessionMemoryContext(context);
  assert(section);

  const assembled = buildProfileRoleAssembly(profileContext, {
    sessionMemoryContext: section,
  });
  const instructions = assembled.roleAssembly.instructions ?? "";
  assert.match(instructions, /# Session Memory/);
  assert.match(instructions, /memory-active-branch/);
  assert.match(instructions, /memory-session-choice/);
  assert.match(instructions, /Active branch is investigating/);
  assert.match(instructions, /Keep sibling branch lore out/);
  assert.doesNotMatch(instructions, /memory-sibling-branch/);
  assert.match(instructions, /sibling_branch=1/);
  assert.match(instructions, /policy_disabled=1/);
  assert.match(instructions, /limit_exceeded=1/);
  assertOrder(instructions, [
    "# Profile",
    "# Profile Instructions",
    "# Session Memory",
    "# Runtime",
  ]);

  console.log(
    JSON.stringify(
      {
        ok: true,
        selected: context.diagnostics.selected_records.map(
          (record) => record.record_id,
        ),
        excluded: context.diagnostics.excluded_counts,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}

function sessionMemoryRecord(input: {
  record_id: string;
  shape_id: string;
  scope_type: string;
  scope_id: string;
  branch_id: string | null;
  content: unknown;
}): NativeSessionMemoryPromptContext["records"][number] {
  return {
    record_id: input.record_id,
    session_id: "session-alpha",
    branch_id: input.branch_id,
    scope: {
      scope_type: input.scope_type,
      scope_id: input.scope_id,
    },
    shape: { shape_id: input.shape_id, version: 1 },
    status: "active",
    revision: 1,
    content: input.content,
    evidence_refs: [],
    source: "agent",
    confidence: 0.9,
    durability_rationale: "smoke fixture",
    supersedes_record_id: null,
    superseded_by_record_id: null,
    archived_at: null,
    archive_reason: null,
    created_at: "2026-06-26T00:00:00Z",
    updated_at: "2026-06-26T00:00:00Z",
  };
}

function assertOrder(body: string, expected: readonly string[]): void {
  let cursor = -1;
  for (const needle of expected) {
    const index = body.indexOf(needle);
    assert.notEqual(index, -1, `missing ${needle}`);
    assert(index > cursor, `${needle} appeared out of order`);
    cursor = index;
  }
}
