import assert from "node:assert/strict";
import {
  existsSync,
  mkdtempSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  resolveSkillsTools,
  skillManageTool,
  skillsListTool,
  skillViewTool,
} from "./index.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-skills-tools-"));
const skillsDir = join(root, "skills");
mkdirSync(skillsDir, { recursive: true });

try {
  writeFileSync(
    join(skillsDir, "repo-orientation.md"),
    `---
title: Repo Orientation
summary: Learn the repository shape.
tags:
  - repo
  - orientation
---

Read README and docs before changing code.
`,
  );
  writeFileSync(
    join(skillsDir, "large-skill.md"),
    `---
title: Large Skill
---

${"x".repeat(80)}
`,
  );
  mkdirSync(join(skillsDir, "autonomous-ai-agents", "codex"), {
    recursive: true,
  });
  writeFileSync(
    join(skillsDir, "autonomous-ai-agents", "codex", "SKILL.md"),
    `---
name: codex
description: Delegate coding work through Codex CLI.
tags:
  - coding
---

Use Codex for bounded coding delegation.
`,
  );
  writeFileSync(
    join(skillsDir, "broken.md"),
    `---
title: Broken
bad frontmatter line
---

broken
`,
  );
  writeFileSync(join(skillsDir, "unsafe.name.md"), "ignored");

  const context = {
    skillsDir,
    maxBodyChars: 20,
  };
  const tools = resolveSkillsTools(context);
  assert.deepEqual(
    tools.map((tool) => tool.name),
    ["skills_list", "skill_view"],
  );
  assert.deepEqual(
    resolveSkillsTools({ ...context, manageMode: "profile" }).map(
      (tool) => tool.name,
    ),
    ["skills_list", "skill_view", "skill_manage"],
  );

  const listed = await skillsListTool(context).execute("list", {
    includeInvalid: true,
  });
  assert.equal(listed.details.ok, true);
  assert.deepEqual(
    listed.details.skills?.map((skill) => [skill.slug, skill.status]),
    [
      ["broken", "invalid"],
      ["codex", "available"],
      ["large-skill", "available"],
      ["repo-orientation", "available"],
    ],
  );

  const hiddenInvalid = await skillsListTool(context).execute("list", {});
  assert.deepEqual(
    hiddenInvalid.details.skills?.map((skill) => skill.slug),
    ["codex", "large-skill", "repo-orientation"],
  );

  const viewed = await skillViewTool(context).execute("view", {
    slug: "repo-orientation",
  });
  assert.equal(viewed.details.ok, true);
  assert.equal(viewed.details.skill?.title, "Repo Orientation");
  assert.match(viewed.details.skill?.bodyMarkdown ?? "", /Read README/);

  const viewedNested = await skillViewTool(context).execute("view-nested", {
    slug: "codex",
  });
  assert.equal(
    viewedNested.details.skill?.summary,
    "Delegate coding work through Codex CLI.",
  );
  assert.match(viewedNested.details.skill?.sourcePath ?? "", /SKILL\.md$/);

  const truncated = await skillViewTool(context).execute("view-large", {
    slug: "large-skill",
  });
  assert.equal(truncated.details.skill?.truncated, true);
  assert.equal(truncated.details.skill?.bodyMarkdown?.length, 20);

  const noBody = await skillViewTool(context).execute("view-no-body", {
    slug: "repo-orientation",
    includeBody: false,
  });
  assert.equal(noBody.details.skill?.bodyMarkdown, undefined);
  assert.equal(noBody.details.skill?.bodyChars, 42);

  const denied = await skillViewTool({
    skillsDir,
    allowedSkills: ["repo-orientation"],
  }).execute("view-denied", {
    slug: "large-skill",
  });
  assert.equal(denied.details.ok, false);
  assert.equal(denied.details.reasonCode, "skill_not_allowed");

  const missingRoot = await skillsListTool({}).execute("missing-root", {});
  assert.equal(missingRoot.details.ok, false);
  assert.equal(missingRoot.details.reasonCode, "skills_root_missing");

  const disabledManage = await skillManageTool({ skillsDir }).execute(
    "manage-disabled",
    {
      action: "create",
      slug: "new-skill",
      content: "new body",
    },
  );
  assert.equal(disabledManage.details.ok, false);
  assert.equal(disabledManage.details.reasonCode, "skill_manage_disabled");

  const managedContext = {
    skillsDir,
    manageMode: "profile" as const,
    now: () => new Date("2026-06-20T12:00:00.000Z"),
  };
  const managedTool = skillManageTool(managedContext);
  const created = await managedTool.execute("create", {
    action: "create",
    slug: "managed",
    content: `---
title: Managed
tags:
  - managed
---

Original managed body.
`,
    provenance: "smoke",
  });
  assert.equal(created.details.ok, true);
  assert.equal(created.details.management?.changed, true);
  assert.equal(existsSync(join(skillsDir, "managed.md")), true);

  const duplicateCreate = await managedTool.execute("duplicate-create", {
    action: "create",
    slug: "managed",
    content: "duplicate",
  });
  assert.equal(duplicateCreate.details.ok, false);
  assert.equal(duplicateCreate.details.reasonCode, "skill_already_exists");

  const patched = await managedTool.execute("patch", {
    action: "patch",
    slug: "managed",
    old_string: "Original managed body.",
    new_string: "Patched managed body.",
  });
  assert.equal(patched.details.ok, true);
  assert.equal(patched.details.management?.oldStringMatches, 1);
  assert.match(readFileSync(join(skillsDir, "managed.md"), "utf8"), /Patched/);

  const notUnique = await managedTool.execute("patch-not-unique", {
    action: "patch",
    slug: "managed",
    old_string: "managed",
    new_string: "skill",
  });
  assert.equal(notUnique.details.ok, false);
  assert.equal(notUnique.details.reasonCode, "old_string_not_unique");

  const wroteSidecar = await managedTool.execute("write-sidecar", {
    action: "write_file",
    slug: "managed",
    file_path: "references/note.md",
    file_content: "sidecar reference",
  });
  assert.equal(wroteSidecar.details.ok, true);
  assert.equal(
    existsSync(join(skillsDir, "managed.d", "references", "note.md")),
    true,
  );

  const pathTraversal = await managedTool.execute("write-traversal", {
    action: "write_file",
    slug: "managed",
    file_path: "../escape.md",
    file_content: "bad",
  });
  assert.equal(pathTraversal.details.ok, false);
  assert.equal(pathTraversal.details.reasonCode, "invalid_file_path");

  writeFileSync(join(skillsDir, "managed.pinned"), "");
  const pinnedDelete = await managedTool.execute("delete-pinned", {
    action: "delete",
    slug: "managed",
    absorbed_into: "repo-orientation",
  });
  assert.equal(pinnedDelete.details.ok, false);
  assert.equal(pinnedDelete.details.reasonCode, "skill_pinned");
  rmSync(join(skillsDir, "managed.pinned"));

  const missingAbsorbedInto = await managedTool.execute("delete-missing", {
    action: "delete",
    slug: "managed",
  });
  assert.equal(missingAbsorbedInto.details.ok, false);
  assert.equal(missingAbsorbedInto.details.reasonCode, "missing_absorbed_into");

  const deleted = await managedTool.execute("delete", {
    action: "delete",
    slug: "managed",
    absorbed_into: "repo-orientation",
    provenance: "smoke",
  });
  assert.equal(deleted.details.ok, true);
  assert.equal(existsSync(join(skillsDir, "managed.md")), false);
  assert.equal(
    existsSync(
      join(skillsDir, ".archive", "managed-2026-06-20T12-00-00-000Z.md"),
    ),
    true,
  );
  assert.equal(
    existsSync(
      join(skillsDir, ".archive", "managed-2026-06-20T12-00-00-000Z.d"),
    ),
    true,
  );

  console.log(
    JSON.stringify(
      {
        tools: tools.map((tool) => tool.name),
        listed: listed.details.skills?.length,
        visible: hiddenInvalid.details.skills?.length,
        viewed: viewed.details.skill?.slug,
        viewedNested: viewedNested.details.skill?.slug,
        truncated: truncated.details.skill?.truncated,
        denied: denied.details.reasonCode,
        missingRoot: missingRoot.details.reasonCode,
        manageDisabled: disabledManage.details.reasonCode,
        created: created.details.management?.changed,
        duplicateCreate: duplicateCreate.details.reasonCode,
        patched: patched.details.management?.oldStringMatches,
        pathTraversal: pathTraversal.details.reasonCode,
        pinnedDelete: pinnedDelete.details.reasonCode,
        deleted: deleted.details.management?.archivePath !== undefined,
      },
      null,
      2,
    ),
  );
} finally {
  rmSync(root, { recursive: true, force: true });
}
