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

export interface CuratorAdminStatus {
  status: "available" | "paused" | "degraded";
  candidateCount?: number;
  mutationCount?: number;
  lastRunAt?: string;
  lastError?: string;
}

export interface CuratorAdminControlOptions {
  curatorExecutor: (
    request: CuratorExecuteRequest,
  ) => Promise<CuratorExecuteReceipt> | CuratorExecuteReceipt;
  status?: () => Promise<CuratorAdminStatus> | CuratorAdminStatus;
  rollbackMutation?: (
    mutationId: string,
  ) => Promise<CuratorMutationRecord> | CuratorMutationRecord;
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
