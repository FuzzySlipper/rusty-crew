import type {
  AgentActivityObservationProducer,
  AgentActivityPublishResult,
} from "./agent-activity-observation.js";
import {
  sessionActivity,
  type AgentObservationIdentity,
} from "./agent-activity-observation.js";
import type {
  AdminControlCommand,
  AdminControlExecutor,
  AdminControlOutcome,
} from "./admin-control-api.js";

export type NewSessionLifecyclePhase =
  | "template_loaded"
  | "archive_started"
  | "archived"
  | "create_started"
  | "created"
  | "binding_rebind_started"
  | "binding_rebound";

export interface NewSessionTemplate {
  agentId: string;
  profileId: string;
  kind: "full" | "worker" | "delegated";
  channelBindingId?: string;
  channelId?: string | number;
  toolProfileKey?: string;
  sessionConfig?: Record<string, unknown>;
}

export interface NewSessionLifecycleAuditEvent {
  phase: NewSessionLifecyclePhase;
  oldSessionId: string;
  newSessionId?: string;
  reason: string;
  reasonCode: string;
  observedAt: string;
}

export interface NewSessionLifecycleAuditSink {
  writeNewSessionLifecycleAudit(
    event: NewSessionLifecycleAuditEvent,
  ): Promise<unknown> | unknown;
}

export interface NewSessionLifecycleOptions {
  loadTemplate(
    currentSessionId: string,
    command: AdminControlCommand,
  ): Promise<NewSessionTemplate> | NewSessionTemplate;
  generateSessionId(
    template: NewSessionTemplate,
    command: AdminControlCommand,
  ): string;
  archiveSession(input: {
    sessionId: string;
    reason: string;
    reasonCode: string;
    command: AdminControlCommand;
  }): Promise<unknown> | unknown;
  createSession(input: {
    sessionId: string;
    template: NewSessionTemplate;
    reason: string;
    reasonCode: string;
    command: AdminControlCommand;
  }): Promise<unknown> | unknown;
  rebindChannel?(input: {
    oldSessionId: string;
    newSessionId: string;
    template: NewSessionTemplate;
    reason: string;
    reasonCode: string;
    command: AdminControlCommand;
  }): Promise<unknown> | unknown;
  auditSink?: NewSessionLifecycleAuditSink;
  observationProducer?: AgentActivityObservationProducer;
  observationIdentity?(input: {
    template: NewSessionTemplate;
    sessionId: string;
    command: AdminControlCommand;
  }): AgentObservationIdentity;
  now?: () => string;
}

export interface MemoryNewSessionLifecycleAuditSink extends NewSessionLifecycleAuditSink {
  readonly events: NewSessionLifecycleAuditEvent[];
}

export function createNewSessionLifecycleExecutor(
  options: NewSessionLifecycleOptions,
): NonNullable<AdminControlExecutor["newSession"]> {
  return async (command) => {
    const oldSessionId = command.target.sessionId;
    const reason = command.reason ?? "slash command /new";
    const reasonCode = command.reasonCode ?? "slash_command_new";
    if (!oldSessionId) {
      return failed(
        "missing_session_id",
        "Cannot create a new session without a current session.",
      );
    }

    const template = await options.loadTemplate(oldSessionId, command);
    await audit(options, {
      phase: "template_loaded",
      oldSessionId,
      reason,
      reasonCode,
    });

    const newSessionId = options.generateSessionId(template, command);
    if (!newSessionId || newSessionId === oldSessionId) {
      return failed(
        "new_session_identity_not_distinct",
        "New session ID must be distinct from the archived session.",
      );
    }

    const requiresRebind = Boolean(
      template.channelBindingId ?? template.channelId,
    );
    if (requiresRebind && !options.rebindChannel) {
      return failed(
        "missing_channel_rebind",
        "Channel binding context requires an explicit rebind handler.",
      );
    }

    await audit(options, {
      phase: "archive_started",
      oldSessionId,
      newSessionId,
      reason,
      reasonCode,
    });
    await options.archiveSession({
      sessionId: oldSessionId,
      reason,
      reasonCode,
      command,
    });
    await audit(options, {
      phase: "archived",
      oldSessionId,
      newSessionId,
      reason,
      reasonCode,
    });
    const archiveObservation = await publishSessionObservation(
      options,
      template,
      oldSessionId,
      command,
      "agent_session_stopped",
      `Archived session ${oldSessionId} for /new.`,
      reasonCode,
    );

    await audit(options, {
      phase: "create_started",
      oldSessionId,
      newSessionId,
      reason,
      reasonCode,
    });
    await options.createSession({
      sessionId: newSessionId,
      template,
      reason,
      reasonCode,
      command,
    });
    await audit(options, {
      phase: "created",
      oldSessionId,
      newSessionId,
      reason,
      reasonCode,
    });
    const createObservation = await publishSessionObservation(
      options,
      template,
      newSessionId,
      command,
      "agent_session_started",
      `Created session ${newSessionId} for /new.`,
      reasonCode,
    );

    if (requiresRebind && options.rebindChannel) {
      await audit(options, {
        phase: "binding_rebind_started",
        oldSessionId,
        newSessionId,
        reason,
        reasonCode,
      });
      await options.rebindChannel({
        oldSessionId,
        newSessionId,
        template,
        reason,
        reasonCode,
        command,
      });
      await audit(options, {
        phase: "binding_rebound",
        oldSessionId,
        newSessionId,
        reason,
        reasonCode,
      });
    }

    return {
      status: "completed",
      summary: `Archived ${oldSessionId} and created ${newSessionId}.`,
      affectedIds: {
        oldSessionId,
        newSessionId,
        ...(template.channelBindingId
          ? { channelBindingId: template.channelBindingId }
          : {}),
      },
      result: {
        oldSessionId,
        newSessionId,
        reattachedChannelBinding: requiresRebind,
        observation: {
          archived: archiveObservation?.status,
          created: createObservation?.status,
        },
      },
      reasonCode,
    } satisfies AdminControlOutcome;
  };
}

export function createMemoryNewSessionLifecycleAuditSink(): MemoryNewSessionLifecycleAuditSink {
  const events: NewSessionLifecycleAuditEvent[] = [];
  return {
    events,
    writeNewSessionLifecycleAudit(event) {
      events.push(event);
    },
  };
}

function failed(reasonCode: string, summary: string): AdminControlOutcome {
  return {
    status: "failed",
    summary,
    reasonCode,
  };
}

async function audit(
  options: NewSessionLifecycleOptions,
  event: Omit<NewSessionLifecycleAuditEvent, "observedAt">,
): Promise<void> {
  await options.auditSink?.writeNewSessionLifecycleAudit({
    ...event,
    observedAt: options.now?.() ?? new Date().toISOString(),
  });
}

async function publishSessionObservation(
  options: NewSessionLifecycleOptions,
  template: NewSessionTemplate,
  sessionId: string,
  command: AdminControlCommand,
  eventType: "agent_session_stopped" | "agent_session_started",
  summary: string,
  reasonCode: string,
): Promise<AgentActivityPublishResult | undefined> {
  if (!options.observationProducer || !options.observationIdentity) {
    return undefined;
  }
  return options.observationProducer.publish(
    sessionActivity({
      eventType,
      identity: options.observationIdentity({ template, sessionId, command }),
      summary,
      reasonCode,
    }),
  );
}
