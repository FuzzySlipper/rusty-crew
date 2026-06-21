import assert from "node:assert/strict";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  createCuratorAdminControlExecutor,
  handleAdminControlRequest,
  type AdminControlResponse,
  type AdminRouteResult,
} from "./index.js";
import { createMemoryAdminControlAuditSink } from "./test-support.js";

const root = mkdtempSync(join(tmpdir(), "rusty-crew-curator-admin-"));
const skillsDir = join(root, "skills");
const archiveDir = join(skillsDir, ".archive");
mkdirSync(archiveDir, { recursive: true });
writeFileSync(
  join(skillsDir, "managed.md"),
  `---
title: Managed
summary: Managed skill.
---

Managed body.
`,
  "utf8",
);
const archivedSkillPath = join(
  archiveDir,
  "archived-skill-2026-06-21T00-00-00-000Z.md",
);
const archivedManifestPath = join(
  archiveDir,
  "archived-skill-2026-06-21T00-00-00-000Z.delete.json",
);
writeFileSync(
  archivedSkillPath,
  `---
title: Archived Skill
summary: Archived fixture.
---

Archived body.
`,
  "utf8",
);
writeFileSync(
  archivedManifestPath,
  `${JSON.stringify(
    {
      slug: "archived-skill",
      action: "delete",
      archivedAt: "2026-06-21T00-00-00-000Z",
      absorbed_into: "managed",
      skillArchivePath: archivedSkillPath,
    },
    null,
    2,
  )}\n`,
  "utf8",
);

const auditSink = createMemoryAdminControlAuditSink();
const executor = createCuratorAdminControlExecutor({
  skillsDir,
  curatorExecutor(request) {
    return {
      receiptId: "curator-admin-smoke",
      status: request.action === "apply_candidate" ? "applied" : "requested",
      candidateId: request.candidateId,
      summary: `Curator ${request.action} completed.`,
    };
  },
});
const context = {
  auth: { bearerToken: "control-token", operatorId: "operator-alpha" },
  executor,
  auditSink,
  now: () => "2026-06-21T00:00:00.000Z",
};

const pin = await post("/v1/admin/control/curator/skills/managed/pin", {
  reason: "operator protects this skill",
});
assert.equal(pin.command.name, "curator_pin_skill");
assert.equal(existsSync(join(skillsDir, "managed.pinned")), true);

const pinned = await post("/v1/admin/control/curator/pinned/list", {});
assert.equal(pinned.command.name, "curator_list_pinned_skills");
assert.match(JSON.stringify(pinned.outcome.result), /managed/);

const unpin = await post("/v1/admin/control/curator/skills/managed/unpin", {});
assert.equal(unpin.command.name, "curator_unpin_skill");
assert.equal(existsSync(join(skillsDir, "managed.pinned")), false);

const archived = await post("/v1/admin/control/curator/archives/list", {});
assert.equal(archived.command.name, "curator_list_archived_skills");
assert.match(JSON.stringify(archived.outcome.result), /archived-skill/);

const restored = await post(
  "/v1/admin/control/curator/skills/archived-skill/restore",
  {},
);
assert.equal(restored.command.name, "curator_restore_skill");
assert.equal(existsSync(join(skillsDir, "archived-skill.md")), true);
assert.match(
  readFileSync(join(skillsDir, "archived-skill.md"), "utf8"),
  /Archived body/,
);
assert.equal(auditSink.events.length, 10);

console.log("curator admin control smoke passed");

async function post(
  url: string,
  body: Record<string, unknown>,
): Promise<AdminControlResponse> {
  const result = await handleAdminControlRequest(
    {
      method: "POST",
      url,
      headers: {
        authorization: "Bearer control-token",
        "x-rusty-crew-operator": "operator-alpha",
      },
      body,
    },
    context,
  );
  assert.equal(result.status, 200);
  return okData(result);
}

function okData<T>(result: AdminRouteResult): T {
  assert.equal(result.body.ok, true);
  return result.body.data as T;
}
