import type { SessionId, SessionState } from "@rusty-crew/contracts";
import type { AdminControlResponse } from "./admin-control-api.js";
import type { RuntimeDiagnosticsProjection } from "./runtime-diagnostics.js";
import type {
  ChatEvent,
  ExecuteChatCommandInput,
  ExecuteChatCommandResult,
  SessionContextUsageResult,
} from "./rusty-view-chat-api.js";
import { buildReadOnlySlashCommandResponse } from "./slash-command-responses.js";
import {
  routeSlashCommand,
  type SlashCommandSession,
} from "./slash-command-router.js";

export interface RustyViewSlashCommandContext {
  appendChatEvent(
    sessionId: SessionId,
    event: Pick<ChatEvent, "kind" | "payload">,
  ): Promise<ChatEvent>;
  buildDiagnosticsContext(): Promise<{
    diagnostics: RuntimeDiagnosticsProjection;
  }>;
  sessionContextUsage(input: {
    session: SessionState;
    requestId: string;
  }): Promise<SessionContextUsageResult>;
  executeControlCommand(input: {
    commandName: string;
    sessionId: SessionId;
    actorId: string;
    body: Record<string, unknown>;
    requestId: string;
  }): Promise<{
    controlStatus: number;
    outcome: AdminControlResponse["outcome"];
  }>;
}

export async function executeRustyViewChatCommand(
  context: RustyViewSlashCommandContext,
  input: ExecuteChatCommandInput,
): Promise<ExecuteChatCommandResult> {
  const started = await context.appendChatEvent(input.session.sessionId, {
    kind: "command_started",
    payload: {
      command: input.command,
      actor: input.actor,
      request_id: input.requestId,
    },
  });
  const routed = routeSlashCommand({
    text: input.command,
    session: slashCommandSession(input.session),
    actor: {
      id: input.actor.id,
      displayName: input.actor.display_name,
    },
    options: {
      primeProfiles: [input.session.profileId],
      allowNonPrimeReadCommands: true,
    },
  });
  if (routed.kind === "pass_through") {
    return completeChatCommand(context, input.session.sessionId, {
      status: "rejected",
      command_name: "unknown",
      summary:
        "Only slash commands can be executed through the chat command API.",
      latest_cursor: started.event_id,
      reason_code: "not_a_slash_command",
    });
  }
  if (routed.status !== "ok") {
    return completeChatCommand(context, input.session.sessionId, {
      status: "rejected",
      command_name: routed.commandName,
      summary: routed.response.summary,
      latest_cursor: started.event_id,
      reason_code:
        routed.status === "denied" ? "slash_command_denied" : "unknown_command",
      response: routed.response,
    });
  }
  if (
    routed.commandName === "help" ||
    routed.commandName === "status" ||
    routed.commandName === "session" ||
    routed.commandName === "model" ||
    (routed.commandName === "effort" && !routed.controlRequest)
  ) {
    const diagnosticsContext = await context.buildDiagnosticsContext();
    const modelContext =
      routed.commandName === "model" || routed.commandName === "effort"
        ? await context.sessionContextUsage({
            session: input.session,
            requestId: input.requestId,
          })
        : undefined;
    const response = buildReadOnlySlashCommandResponse(routed.commandName, {
      diagnostics: diagnosticsContext.diagnostics,
      session: slashCommandSession(input.session),
      modelContext,
      options: {
        primeProfiles: [input.session.profileId],
        allowNonPrimeReadCommands: true,
      },
    });
    return completeChatCommand(context, input.session.sessionId, {
      status: "completed",
      command_name: routed.commandName,
      summary: response.summary,
      latest_cursor: started.event_id,
      response,
    });
  }
  if (routed.controlRequest) {
    const control = await context.executeControlCommand({
      commandName: routed.controlRequest.commandName,
      sessionId: input.session.sessionId,
      actorId: input.actor.id,
      body: {
        ...routed.controlRequest.body,
        reason: routed.controlRequest.reason,
        reasonCode: routed.controlRequest.reasonCode,
        ...(routed.commandName === "archive"
          ? { chatCommandName: "archive" }
          : {}),
      },
      requestId: input.requestId,
    });
    const outcome = control.outcome;
    const affected = outcome.affectedIds ?? {};
    const result: ExecuteChatCommandResult = {
      status: outcome.status === "completed" ? "completed" : "failed",
      command_name: routed.commandName,
      summary: outcome.summary,
      latest_cursor: started.event_id,
      old_session_id: stringRecordValue(affected, "oldSessionId"),
      new_session_id: stringRecordValue(affected, "newSessionId"),
      reason_code: outcome.reasonCode,
      response: { outcome, control_status: control.controlStatus },
    };
    const commandEventCursor = nestedString(
      outcome.result,
      "commandEventCursor",
    );
    return commandEventCursor === undefined
      ? completeChatCommand(context, input.session.sessionId, result)
      : { ...result, latest_cursor: commandEventCursor };
  }
  return completeChatCommand(context, input.session.sessionId, {
    status: "failed",
    command_name: routed.commandName,
    summary: "Slash command did not produce an executable action.",
    latest_cursor: started.event_id,
    reason_code: "missing_command_action",
  });
}

async function completeChatCommand(
  context: RustyViewSlashCommandContext,
  sessionId: SessionId,
  result: ExecuteChatCommandResult,
): Promise<ExecuteChatCommandResult> {
  const completed = await context.appendChatEvent(sessionId, {
    kind:
      result.status === "completed" ? "command_completed" : "command_failed",
    payload: { ...result },
  });
  return {
    ...result,
    latest_cursor: completed.event_id,
  };
}

function slashCommandSession(session: SessionState): SlashCommandSession {
  return {
    sessionId: session.sessionId,
    agentId: session.agentId,
    profileId: session.profileId,
    kind: session.kind,
    reasoningEffortOverride:
      session.inferenceOverrides?.reasoningEffort ?? undefined,
  };
}

export function controlUrlForSlashCommand(
  commandName: string,
  sessionId: SessionId,
): string {
  if (commandName === "archive_session") {
    return `/v1/admin/control/sessions/${sessionId}/archive`;
  }
  if (commandName === "new_session") {
    return `/v1/admin/control/sessions/${sessionId}/new`;
  }
  if (commandName === "reload_mcp") {
    return `/v1/admin/control/mcp/${sessionId}/reload`;
  }
  if (commandName === "set_session_effort") {
    return `/v1/admin/control/sessions/${sessionId}/effort`;
  }
  return `/v1/admin/control/unsupported/${commandName}`;
}

function nestedString(value: unknown, key: string): string | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return undefined;
  }
  const candidate = (value as Record<string, unknown>)[key];
  return typeof candidate === "string" ? candidate : undefined;
}

function stringRecordValue(
  record: Record<string, string | number>,
  key: string,
): string | undefined {
  const value = record[key];
  return typeof value === "string" ? value : undefined;
}
