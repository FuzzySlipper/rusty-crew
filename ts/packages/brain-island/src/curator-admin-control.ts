import type {
  CuratorExecuteReceipt,
  CuratorExecuteRequest,
  CuratorScopeType,
} from "./planning-tools.js";
import type {
  AdminControlCommand,
  AdminControlExecutor,
  AdminControlOutcome,
} from "./admin-control-api.js";
import type { CuratorMutationRecord } from "./curator-mutations.js";
import type { CuratorLifecycleReport } from "./curator-lifecycle.js";
import {
  listCuratorArchivedSkills,
  listCuratorPinnedSkills,
  pinCuratorSkill,
  restoreCuratorArchivedSkill,
  unpinCuratorSkill,
} from "./curator-skill-admin.js";

export interface CuratorAdminStatus {
  status: "available" | "paused" | "degraded";
  candidateCount?: number;
  mutationCount?: number;
  pinnedSkillCount?: number;
  archivedSkillCount?: number;
  lastRunAt?: string;
  lastError?: string;
  lifecycle?: CuratorLifecycleReport;
}

export interface CuratorAdminControlOptions {
  curatorExecutor: (
    request: CuratorExecuteRequest,
  ) => Promise<CuratorExecuteReceipt> | CuratorExecuteReceipt;
  status?: () => Promise<CuratorAdminStatus> | CuratorAdminStatus;
  rollbackMutation?: (
    mutationId: string,
  ) => Promise<CuratorMutationRecord> | CuratorMutationRecord;
  skillsDir?: string;
}

export function createCuratorAdminControlExecutor(
  options: CuratorAdminControlOptions,
): Pick<
  AdminControlExecutor,
  | "curatorStatus"
  | "curatorRunScan"
  | "curatorPreviewCandidate"
  | "curatorApproveCandidate"
  | "curatorApplyCandidate"
  | "curatorRollbackMutation"
  | "curatorPinSkill"
  | "curatorUnpinSkill"
  | "curatorRestoreSkill"
  | "curatorListPinnedSkills"
  | "curatorListArchivedSkills"
> {
  return {
    curatorStatus: async () => {
      const status = options.status
        ? await options.status()
        : ({ status: "available" } satisfies CuratorAdminStatus);
      return {
        status: "completed",
        summary: `Curator is ${status.status}.`,
        result: status,
      };
    },
    curatorRunScan: async (command) => {
      const receipt = await options.curatorExecutor({
        action: "request_scan",
        scopeType: scopeType(command),
        scopeId: stringBody(command, "scopeId"),
        actorId: command.actor.operatorId,
        reason: command.reason,
        dryRun: boolBody(command, "dryRun") ?? true,
      });
      return curatorOutcome(command, receipt);
    },
    curatorPreviewCandidate: async (command) => {
      const receipt = await options.curatorExecutor({
        action: "preview_candidate",
        candidateId: command.target.candidateId,
        actorId: command.actor.operatorId,
        reason: command.reason,
        dryRun: true,
      });
      return curatorOutcome(command, receipt);
    },
    curatorApproveCandidate: async (command) => {
      const receipt = await options.curatorExecutor({
        action: "approve_candidate",
        candidateId: command.target.candidateId,
        actorId: command.actor.operatorId,
        reason: command.reason ?? "admin curator approval",
        dryRun: false,
      });
      return curatorOutcome(command, receipt);
    },
    curatorApplyCandidate: async (command) => {
      const receipt = await options.curatorExecutor({
        action: "apply_candidate",
        candidateId: command.target.candidateId,
        actorId: command.actor.operatorId,
        reason: command.reason ?? "admin curator apply",
        dryRun: boolBody(command, "dryRun") ?? false,
      });
      return curatorOutcome(command, receipt);
    },
    curatorRollbackMutation: async (command) => {
      if (!options.rollbackMutation) {
        return {
          status: "failed",
          summary: "Curator rollback is not configured.",
          reasonCode: "curator_rollback_unavailable",
        };
      }
      const mutationId = command.target.mutationId ?? "";
      const record = await options.rollbackMutation(mutationId);
      return {
        status: "completed",
        summary: `Rolled back curator mutation ${mutationId}.`,
        affectedIds: {
          mutationId,
          candidateId: record.candidateId,
        },
        result: {
          mutationId: record.mutationId,
          status: record.status,
          rollbackRef: record.rollbackRef,
        },
      };
    },
    curatorPinSkill: async (command) => {
      const skillsDir = requiredSkillsDir(options);
      const slug = command.target.slug ?? "";
      const result = await pinCuratorSkill({
        skillsDir,
        slug,
        reason: command.reason,
        operatorId: command.actor.operatorId,
      });
      return {
        status: "completed",
        summary: `Pinned skill ${slug}.`,
        affectedIds: { slug },
        result,
      };
    },
    curatorUnpinSkill: async (command) => {
      const skillsDir = requiredSkillsDir(options);
      const slug = command.target.slug ?? "";
      const result = await unpinCuratorSkill({ skillsDir, slug });
      return {
        status: "completed",
        summary: `Unpinned skill ${slug}.`,
        affectedIds: { slug },
        result,
      };
    },
    curatorRestoreSkill: async (command) => {
      const skillsDir = requiredSkillsDir(options);
      const slug = command.target.slug ?? "";
      const result = await restoreCuratorArchivedSkill({
        skillsDir,
        slug,
        manifestPath: stringBody(command, "manifestPath"),
      });
      return {
        status: "completed",
        summary: `Restored archived skill ${slug}.`,
        affectedIds: { slug },
        result,
      };
    },
    curatorListPinnedSkills: async () => {
      const skillsDir = requiredSkillsDir(options);
      const result = await listCuratorPinnedSkills(skillsDir);
      return {
        status: "completed",
        summary: `Found ${result.length} pinned skill(s).`,
        result,
      };
    },
    curatorListArchivedSkills: async () => {
      const skillsDir = requiredSkillsDir(options);
      const result = await listCuratorArchivedSkills(skillsDir);
      return {
        status: "completed",
        summary: `Found ${result.length} archived skill(s).`,
        result,
      };
    },
  };
}

function curatorOutcome(
  command: AdminControlCommand,
  receipt: CuratorExecuteReceipt,
): AdminControlOutcome {
  return {
    status: "completed",
    summary: receipt.summary,
    affectedIds: {
      candidateId: command.target.candidateId ?? receipt.candidateId ?? "",
    },
    result: receipt,
  };
}

function scopeType(command: AdminControlCommand): CuratorScopeType | undefined {
  const value = stringBody(command, "scopeType");
  if (
    value === "profile" ||
    value === "skills_root" ||
    value === "project" ||
    value === "session"
  ) {
    return value;
  }
  return undefined;
}

function stringBody(
  command: AdminControlCommand,
  key: string,
): string | undefined {
  const value = command.body[key];
  return typeof value === "string" && value.trim() ? value : undefined;
}

function boolBody(
  command: AdminControlCommand,
  key: string,
): boolean | undefined {
  const value = command.body[key];
  return typeof value === "boolean" ? value : undefined;
}

function requiredSkillsDir(options: CuratorAdminControlOptions): string {
  if (!options.skillsDir) {
    throw new Error("curator_skill_admin_unavailable");
  }
  return options.skillsDir;
}
