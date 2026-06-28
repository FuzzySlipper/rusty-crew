import {
  adminCommandActivity,
  type AgentActivityObservationProducer,
  type AgentActivityPublishResult,
  type AgentActivityResultRef,
  type AgentObservationIdentity,
} from "./agent-activity-observation.js";
import type {
  AdminApiMeta,
  AdminErrorCode,
  AdminRouteResult,
} from "./admin-diagnostics-api.js";

export type AdminControlCommandName =
  | "create_profile"
  | "read_profile_config"
  | "plan_profile_update"
  | "apply_profile_update"
  | "decommission_profile"
  | "create_session"
  | "archive_session"
  | "new_session"
  | "pause_runtime"
  | "resume_runtime"
  | "plan_runtime_rebuild"
  | "apply_runtime_rebuild"
  | "cancel_delegation"
  | "request_delegated_checkpoint"
  | "reload_config"
  | "plan_runtime_config_update"
  | "apply_runtime_config_update"
  | "reload_mcp"
  | "run_maintenance"
  | "scheduler_tick"
  | "scheduler_run_job"
  | "scheduler_pause_job"
  | "scheduler_resume_job"
  | "cleanup_delegated_resources"
  | "curator_status"
  | "curator_run_scan"
  | "curator_preview_candidate"
  | "curator_approve_candidate"
  | "curator_apply_candidate"
  | "curator_rollback_mutation"
  | "curator_pin_skill"
  | "curator_unpin_skill"
  | "curator_restore_skill"
  | "curator_list_pinned_skills"
  | "curator_list_archived_skills"
  | "shutdown";

export type AdminControlStatus = "completed" | "failed";

export interface AdminControlRouteRequest {
  method: string;
  url: string;
  headers?: Record<string, string | undefined>;
  body?: unknown;
  requestId?: string;
}

export interface AdminControlAuthConfig {
  bearerToken: string;
  operatorId?: string;
}

export interface AdminControlActor {
  operatorId: string;
}

export interface AdminControlCommand {
  name: AdminControlCommandName;
  target: Record<string, string>;
  actor: AdminControlActor;
  requestId: string;
  idempotencyKey?: string;
  reason?: string;
  reasonCode?: string;
  denRefs?: Record<string, string | number>;
  body: Record<string, unknown>;
}

export interface AdminControlOutcome {
  status: AdminControlStatus;
  summary: string;
  affectedIds?: Record<string, string | number>;
  result?: unknown;
  reasonCode?: string;
}

export interface AdminControlAuditEvent {
  phase: "started" | "completed" | "failed";
  command: AdminControlCommand;
  outcome?: AdminControlOutcome;
  observedAt: string;
}

export interface AdminControlAuditSink {
  writeAdminControlAudit(
    event: AdminControlAuditEvent,
  ): Promise<unknown> | unknown;
}

export interface AdminControlExecutor {
  createProfile?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  readProfileConfig?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  planProfileUpdate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  applyProfileUpdate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  decommissionProfile?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  createSession?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  archiveSession?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  newSession?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  pauseRuntime?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  resumeRuntime?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  planRuntimeRebuild?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  applyRuntimeRebuild?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  cancelDelegation?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  requestDelegatedCheckpoint?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  reloadConfig?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  planRuntimeConfigUpdate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  applyRuntimeConfigUpdate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  reloadMcp?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  runMaintenance?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  schedulerTick?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  schedulerRunJob?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  schedulerPauseJob?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  schedulerResumeJob?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  cleanupDelegatedResources?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorStatus?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorRunScan?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorPreviewCandidate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorApproveCandidate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorApplyCandidate?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorRollbackMutation?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorPinSkill?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorUnpinSkill?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorRestoreSkill?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorListPinnedSkills?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  curatorListArchivedSkills?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
  shutdown?(
    command: AdminControlCommand,
  ): Promise<AdminControlOutcome> | AdminControlOutcome;
}

export interface AdminControlContext {
  auth: AdminControlAuthConfig;
  executor: AdminControlExecutor;
  auditSink: AdminControlAuditSink;
  observationProducer?: AgentActivityObservationProducer;
  observationIdentity?: AgentObservationIdentity;
  now?: () => string;
}

export interface AdminControlResponse {
  command: Omit<AdminControlCommand, "body">;
  outcome: AdminControlOutcome;
  audit: {
    started: true;
    terminal: true;
  };
  observation: {
    started?: AgentActivityPublishResult["status"];
    terminal?: AgentActivityPublishResult["status"];
  };
}

export interface MemoryAdminControlAuditSink extends AdminControlAuditSink {
  readonly events: AdminControlAuditEvent[];
  failNext(error?: Error): void;
}

export async function handleAdminControlRequest(
  request: AdminControlRouteRequest,
  context: AdminControlContext,
): Promise<AdminRouteResult> {
  const requestId = request.requestId ?? "admin-control";
  if (request.method.toUpperCase() !== "POST") {
    return failure(405, requestId, {
      code: "method_not_allowed",
      reason_code: "control_requires_post",
      message: "admin control routes only support POST",
      retryable: false,
    });
  }

  const auth = authenticateControlRequest(request, context.auth);
  if (!auth.ok) return auth.result;

  const parsed = parseControlCommand(request, auth.actor, requestId);
  if (!parsed.ok) return parsed.result;

  const executor = executorForCommand(context.executor, parsed.command.name);
  if (!executor) {
    return failure(412, requestId, {
      code: "failed_precondition",
      reason_code: "unsupported_control",
      message: `control ${parsed.command.name} is not configured`,
      retryable: false,
    });
  }

  const observedAt = context.now?.() ?? new Date().toISOString();
  const startedAudit: AdminControlAuditEvent = {
    phase: "started",
    command: parsed.command,
    observedAt,
  };

  try {
    await context.auditSink.writeAdminControlAudit(startedAudit);
  } catch (error) {
    return failure(412, requestId, {
      code: "failed_precondition",
      reason_code: "audit_unavailable",
      message: errorMessage(error, "admin control audit sink is unavailable"),
      retryable: true,
    });
  }

  const startedObservation = await publishControlObservation(
    context,
    parsed.command,
    "admin_command_started",
    `Admin control ${parsed.command.name} started.`,
  );

  let outcome: AdminControlOutcome;
  try {
    outcome = await executor(parsed.command);
  } catch (error) {
    outcome = {
      status: "failed",
      summary: errorMessage(error, `control ${parsed.command.name} failed`),
      reasonCode: "control_executor_failed",
    };
  }

  const terminalPhase = outcome.status === "completed" ? "completed" : "failed";
  try {
    await context.auditSink.writeAdminControlAudit({
      phase: terminalPhase,
      command: parsed.command,
      outcome,
      observedAt: context.now?.() ?? new Date().toISOString(),
    });
  } catch (error) {
    return failure(500, requestId, {
      code: "internal_error",
      reason_code: "terminal_audit_failed",
      message: errorMessage(error, "admin control terminal audit failed"),
      retryable: true,
    });
  }

  const terminalObservation = await publishControlObservation(
    context,
    parsed.command,
    terminalPhase === "completed"
      ? "admin_command_completed"
      : "admin_command_failed",
    outcome.summary,
    outcome.reasonCode,
    adminControlResultRef(parsed.command, outcome),
  );

  return success(requestId, 200, {
    command: publicCommand(parsed.command),
    outcome,
    audit: { started: true, terminal: true },
    observation: {
      started: startedObservation?.status,
      terminal: terminalObservation?.status,
    },
  } satisfies AdminControlResponse);
}

export function createMemoryAdminControlAuditSink(): MemoryAdminControlAuditSink {
  const events: AdminControlAuditEvent[] = [];
  let nextError: Error | undefined;
  return {
    events,
    failNext(error = new Error("admin audit unavailable")) {
      nextError = error;
    },
    writeAdminControlAudit(event) {
      if (nextError) {
        const error = nextError;
        nextError = undefined;
        throw error;
      }
      events.push(event);
    },
  };
}

function authenticateControlRequest(
  request: AdminControlRouteRequest,
  auth: AdminControlAuthConfig,
):
  | { ok: true; actor: AdminControlActor }
  | { ok: false; result: AdminRouteResult } {
  const requestId = request.requestId ?? "admin-control";
  const authorization = header(request.headers, "authorization");
  if (authorization !== `Bearer ${auth.bearerToken}`) {
    return {
      ok: false,
      result: failure(401, requestId, {
        code: "unauthorized",
        reason_code: "missing_or_invalid_bearer_token",
        message: "admin control requires a valid bearer token",
        retryable: false,
      }),
    };
  }

  const operatorId =
    header(request.headers, "x-rusty-crew-operator") ?? auth.operatorId;
  if (!operatorId) {
    return {
      ok: false,
      result: failure(403, requestId, {
        code: "forbidden",
        reason_code: "missing_operator_identity",
        message: "admin control requires an operator identity",
        retryable: false,
      }),
    };
  }

  return { ok: true, actor: { operatorId } };
}

function parseControlCommand(
  request: AdminControlRouteRequest,
  actor: AdminControlActor,
  requestId: string,
):
  | { ok: true; command: AdminControlCommand }
  | { ok: false; result: AdminRouteResult } {
  const url = new URL(request.url, "http://rusty-crew.local");
  const body = normalizeBody(request.body);
  if (!body.ok) {
    return {
      ok: false,
      result: failure(400, requestId, {
        code: "invalid_input",
        reason_code: "invalid_control_body",
        message: "admin control body must be a JSON object",
        retryable: false,
      }),
    };
  }

  const parts = url.pathname.split("/").filter(Boolean);
  const commandBase = {
    actor,
    requestId,
    idempotencyKey: header(request.headers, "idempotency-key"),
    reason: stringValue(body.value.reason),
    reasonCode: stringValue(body.value.reasonCode),
    denRefs: recordValue(body.value.denRefs),
    body: body.value,
  };

  if (url.pathname === "/v1/admin/control/sessions") {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "create_session",
        target: {},
      },
    };
  }

  if (url.pathname === "/v1/admin/control/profiles") {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "create_profile",
        target: {},
      },
    };
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "profiles" &&
    parts[5] === "decommission"
  ) {
    const profileId = parts[4] ?? "";
    if (!profileId) return invalidTarget(requestId, "missing_profile_id");
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "decommission_profile",
        target: { profileId },
      },
    };
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "profiles" &&
    parts[5] === "read"
  ) {
    const profileId = parts[4] ?? "";
    if (!profileId) return invalidTarget(requestId, "missing_profile_id");
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "read_profile_config",
        target: { profileId },
      },
    };
  }

  if (
    parts.length === 7 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "profiles" &&
    parts[5] === "update"
  ) {
    const profileId = parts[4] ?? "";
    if (!profileId) return invalidTarget(requestId, "missing_profile_id");
    if (parts[6] === "plan" || parts[6] === "apply") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name:
            parts[6] === "plan"
              ? "plan_profile_update"
              : "apply_profile_update",
          target: { profileId },
        },
      };
    }
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "sessions"
  ) {
    const sessionId = parts[4] ?? "";
    if (!sessionId) return invalidTarget(requestId, "missing_session_id");
    if (parts[5] === "archive") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "archive_session",
          target: { sessionId },
        },
      };
    }
    if (parts[5] === "new") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "new_session",
          target: { sessionId },
        },
      };
    }
  }

  if (
    parts.length === 7 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    (parts[3] === "sessions" ||
      parts[3] === "profiles" ||
      parts[3] === "agents") &&
    parts[5] === "runtime"
  ) {
    const targetId = parts[4] ?? "";
    if (!targetId) {
      return invalidTarget(
        requestId,
        parts[3] === "sessions"
          ? "missing_session_id"
          : parts[3] === "profiles"
            ? "missing_profile_id"
            : "missing_agent_id",
      );
    }
    if (parts[6] === "pause" || parts[6] === "resume") {
      const scope =
        parts[3] === "sessions"
          ? "session"
          : parts[3] === "profiles"
            ? "profile"
            : "agent";
      return {
        ok: true,
        command: {
          ...commandBase,
          name: parts[6] === "pause" ? "pause_runtime" : "resume_runtime",
          target:
            scope === "session"
              ? { scope, sessionId: targetId }
              : scope === "profile"
                ? { scope, profileId: targetId }
                : { scope, agentId: targetId },
        },
      };
    }
  }

  if (
    parts.length === 7 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "sessions" &&
    parts[5] === "rebuild-runtime"
  ) {
    const sessionId = parts[4] ?? "";
    if (!sessionId) return invalidTarget(requestId, "missing_session_id");
    if (parts[6] === "plan" || parts[6] === "apply") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name:
            parts[6] === "plan"
              ? "plan_runtime_rebuild"
              : "apply_runtime_rebuild",
          target: { scope: "session", sessionId },
        },
      };
    }
  }

  if (
    parts.length === 7 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "profiles" &&
    parts[5] === "rebuild-brain"
  ) {
    const profileId = parts[4] ?? "";
    if (!profileId) return invalidTarget(requestId, "missing_profile_id");
    if (parts[6] === "plan" || parts[6] === "apply") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name:
            parts[6] === "plan"
              ? "plan_runtime_rebuild"
              : "apply_runtime_rebuild",
          target: { scope: "profile", profileId },
        },
      };
    }
  }

  if (
    parts.length === 5 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "config" &&
    parts[4] === "reload"
  ) {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "reload_config",
        target: {},
      },
    };
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "config" &&
    parts[4] === "draft"
  ) {
    if (parts[5] === "plan" || parts[5] === "apply") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name:
            parts[5] === "plan"
              ? "plan_runtime_config_update"
              : "apply_runtime_config_update",
          target: {},
        },
      };
    }
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "delegations"
  ) {
    const sessionId = parts[4] ?? "";
    if (!sessionId)
      return invalidTarget(requestId, "missing_delegated_session_id");
    if (parts[5] === "cancel") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "cancel_delegation",
          target: { sessionId },
        },
      };
    }
    if (parts[5] === "checkpoint") {
      const parentSessionId = stringValue(body.value.parentSessionId);
      if (!parentSessionId) {
        return invalidTarget(requestId, "missing_parent_session_id");
      }
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "request_delegated_checkpoint",
          target: {
            sessionId,
            parentSessionId,
          },
        },
      };
    }
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "mcp" &&
    parts[5] === "reload"
  ) {
    const sessionId = parts[4] ?? "";
    if (!sessionId) return invalidTarget(requestId, "missing_session_id");
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "reload_mcp",
        target: { sessionId },
      },
    };
  }

  if (url.pathname === "/v1/admin/control/maintenance") {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "run_maintenance",
        target: {},
      },
    };
  }

  if (
    parts.length >= 5 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "scheduler"
  ) {
    if (parts.length === 5 && parts[4] === "tick") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "scheduler_tick",
          target: {},
        },
      };
    }
    if (parts.length === 7 && parts[4] === "jobs") {
      const jobId = parts[5] ?? "";
      if (!jobId) return invalidTarget(requestId, "missing_job_id");
      const action = parts[6];
      if (action === "run") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "scheduler_run_job",
            target: { jobId },
          },
        };
      }
      if (action === "pause") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "scheduler_pause_job",
            target: { jobId },
          },
        };
      }
      if (action === "resume") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "scheduler_resume_job",
            target: { jobId },
          },
        };
      }
    }
  }

  if (
    parts.length === 6 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "cleanup" &&
    parts[4] === "delegated" &&
    parts[5] === "run"
  ) {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "cleanup_delegated_resources",
        target: {},
      },
    };
  }

  if (
    parts.length >= 5 &&
    parts[0] === "v1" &&
    parts[1] === "admin" &&
    parts[2] === "control" &&
    parts[3] === "curator"
  ) {
    if (parts.length === 5 && parts[4] === "status") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "curator_status",
          target: {},
        },
      };
    }
    if (parts.length === 5 && parts[4] === "run") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "curator_run_scan",
          target: {},
        },
      };
    }
    if (parts.length === 6 && parts[4] === "pinned" && parts[5] === "list") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "curator_list_pinned_skills",
          target: {},
        },
      };
    }
    if (parts.length === 6 && parts[4] === "archives" && parts[5] === "list") {
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "curator_list_archived_skills",
          target: {},
        },
      };
    }
    if (parts.length === 7 && parts[4] === "skills") {
      const slug = parts[5] ?? "";
      if (!slug) return invalidTarget(requestId, "missing_skill_slug");
      switch (parts[6]) {
        case "pin":
          return {
            ok: true,
            command: {
              ...commandBase,
              name: "curator_pin_skill",
              target: { slug },
            },
          };
        case "unpin":
          return {
            ok: true,
            command: {
              ...commandBase,
              name: "curator_unpin_skill",
              target: { slug },
            },
          };
        case "restore":
          return {
            ok: true,
            command: {
              ...commandBase,
              name: "curator_restore_skill",
              target: { slug },
            },
          };
      }
    }
    if (parts.length === 7 && parts[4] === "candidates") {
      const candidateId = parts[5] ?? "";
      if (!candidateId) return invalidTarget(requestId, "missing_candidate_id");
      const action = parts[6];
      if (action === "preview") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "curator_preview_candidate",
            target: { candidateId },
          },
        };
      }
      if (action === "approve") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "curator_approve_candidate",
            target: { candidateId },
          },
        };
      }
      if (action === "apply") {
        return {
          ok: true,
          command: {
            ...commandBase,
            name: "curator_apply_candidate",
            target: { candidateId },
          },
        };
      }
    }
    if (
      parts.length === 7 &&
      parts[4] === "mutations" &&
      parts[6] === "rollback"
    ) {
      const mutationId = parts[5] ?? "";
      if (!mutationId) return invalidTarget(requestId, "missing_mutation_id");
      return {
        ok: true,
        command: {
          ...commandBase,
          name: "curator_rollback_mutation",
          target: { mutationId },
        },
      };
    }
  }

  if (url.pathname === "/v1/admin/control/shutdown") {
    return {
      ok: true,
      command: {
        ...commandBase,
        name: "shutdown",
        target: {},
      },
    };
  }

  return {
    ok: false,
    result: failure(404, requestId, {
      code: "not_found",
      reason_code: "unknown_admin_control_route",
      message: `unknown admin control route ${url.pathname}`,
      retryable: false,
    }),
  };
}

function executorForCommand(
  executor: AdminControlExecutor,
  command: AdminControlCommandName,
):
  | ((
      command: AdminControlCommand,
    ) => Promise<AdminControlOutcome> | AdminControlOutcome)
  | undefined {
  switch (command) {
    case "create_profile":
      return executor.createProfile;
    case "read_profile_config":
      return executor.readProfileConfig;
    case "plan_profile_update":
      return executor.planProfileUpdate;
    case "apply_profile_update":
      return executor.applyProfileUpdate;
    case "decommission_profile":
      return executor.decommissionProfile;
    case "create_session":
      return executor.createSession;
    case "archive_session":
      return executor.archiveSession;
    case "new_session":
      return executor.newSession;
    case "pause_runtime":
      return executor.pauseRuntime;
    case "resume_runtime":
      return executor.resumeRuntime;
    case "plan_runtime_rebuild":
      return executor.planRuntimeRebuild;
    case "apply_runtime_rebuild":
      return executor.applyRuntimeRebuild;
    case "cancel_delegation":
      return executor.cancelDelegation;
    case "request_delegated_checkpoint":
      return executor.requestDelegatedCheckpoint;
    case "reload_config":
      return executor.reloadConfig;
    case "plan_runtime_config_update":
      return executor.planRuntimeConfigUpdate;
    case "apply_runtime_config_update":
      return executor.applyRuntimeConfigUpdate;
    case "reload_mcp":
      return executor.reloadMcp;
    case "run_maintenance":
      return executor.runMaintenance;
    case "scheduler_tick":
      return executor.schedulerTick;
    case "scheduler_run_job":
      return executor.schedulerRunJob;
    case "scheduler_pause_job":
      return executor.schedulerPauseJob;
    case "scheduler_resume_job":
      return executor.schedulerResumeJob;
    case "cleanup_delegated_resources":
      return executor.cleanupDelegatedResources;
    case "curator_status":
      return executor.curatorStatus;
    case "curator_run_scan":
      return executor.curatorRunScan;
    case "curator_preview_candidate":
      return executor.curatorPreviewCandidate;
    case "curator_approve_candidate":
      return executor.curatorApproveCandidate;
    case "curator_apply_candidate":
      return executor.curatorApplyCandidate;
    case "curator_rollback_mutation":
      return executor.curatorRollbackMutation;
    case "curator_pin_skill":
      return executor.curatorPinSkill;
    case "curator_unpin_skill":
      return executor.curatorUnpinSkill;
    case "curator_restore_skill":
      return executor.curatorRestoreSkill;
    case "curator_list_pinned_skills":
      return executor.curatorListPinnedSkills;
    case "curator_list_archived_skills":
      return executor.curatorListArchivedSkills;
    case "shutdown":
      return executor.shutdown;
  }
}

async function publishControlObservation(
  context: AdminControlContext,
  command: AdminControlCommand,
  eventType:
    | "admin_command_started"
    | "admin_command_completed"
    | "admin_command_failed",
  summary: string,
  reasonCode?: string,
  resultRef?: AgentActivityResultRef,
): Promise<AgentActivityPublishResult | undefined> {
  if (!context.observationProducer || !context.observationIdentity) {
    return undefined;
  }
  return context.observationProducer.publish(
    adminCommandActivity({
      eventType,
      identity: context.observationIdentity,
      commandName: command.name,
      summary,
      reasonCode,
      resultRef,
    }),
  );
}

function adminControlResultRef(
  command: AdminControlCommand,
  outcome: AdminControlOutcome,
): AgentActivityResultRef | undefined {
  if (
    outcome.status !== "completed" ||
    (command.name !== "plan_runtime_rebuild" &&
      command.name !== "apply_runtime_rebuild")
  ) {
    return undefined;
  }
  return {
    artifact_path: `runtime://admin-control/${command.name}/${command.requestId}`,
  };
}

function success<T>(
  requestId: string,
  status: number,
  data: T,
): AdminRouteResult<T> {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: true,
      data: redactAdminControlData(data),
      meta: meta(requestId),
    },
  };
}

function failure(
  status: number,
  requestId: string,
  error: {
    code: AdminErrorCode;
    reason_code: string;
    message: string;
    retryable: boolean;
  },
): AdminRouteResult {
  return {
    status,
    headers: { "content-type": "application/json" },
    body: {
      ok: false,
      error,
      meta: meta(requestId),
    },
  };
}

function meta(requestId: string): AdminApiMeta {
  return {
    request_id: requestId,
    schema_version: 1,
  };
}

function publicCommand(
  command: AdminControlCommand,
): Omit<AdminControlCommand, "body"> {
  const { body: _body, ...publicShape } = command;
  return publicShape;
}

function normalizeBody(
  body: unknown,
): { ok: true; value: Record<string, unknown> } | { ok: false } {
  if (body === undefined || body === null) return { ok: true, value: {} };
  if (typeof body === "string") {
    try {
      const parsed = JSON.parse(body) as unknown;
      return normalizeBody(parsed);
    } catch {
      return { ok: false };
    }
  }
  if (typeof body !== "object" || Array.isArray(body)) return { ok: false };
  return { ok: true, value: body as Record<string, unknown> };
}

function header(
  headers: Record<string, string | undefined> | undefined,
  name: string,
): string | undefined {
  const match = Object.entries(headers ?? {}).find(
    ([key]) => key.toLowerCase() === name.toLowerCase(),
  );
  return match?.[1];
}

function invalidTarget(
  requestId: string,
  reasonCode: string,
): { ok: false; result: AdminRouteResult } {
  return {
    ok: false,
    result: failure(400, requestId, {
      code: "invalid_input",
      reason_code: reasonCode,
      message: "admin control target is missing or invalid",
      retryable: false,
    }),
  };
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" && value.length > 0 ? value : undefined;
}

function recordValue(
  value: unknown,
): Record<string, string | number> | undefined {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const entries = Object.entries(value).filter(
    (entry): entry is [string, string | number] =>
      typeof entry[1] === "string" || typeof entry[1] === "number",
  );
  return entries.length > 0 ? Object.fromEntries(entries) : undefined;
}

function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

function redactAdminControlData<T>(data: T): T {
  return redactValue(data) as T;
}

function redactValue(value: unknown): unknown {
  if (typeof value === "string") return value.slice(0, 2_048);
  if (Array.isArray(value)) return value.map(redactValue);
  if (value === null || typeof value !== "object") return value;

  const output: Record<string, unknown> = {};
  for (const [key, nested] of Object.entries(value)) {
    if (isSecretLikeKey(key)) {
      output[key] = "[redacted]";
    } else {
      output[key] = redactValue(nested);
    }
  }
  return output;
}

function isSecretLikeKey(key: string): boolean {
  return /authorization|bearer|credential|password|secret|token|api[_-]?key/i.test(
    key,
  );
}
