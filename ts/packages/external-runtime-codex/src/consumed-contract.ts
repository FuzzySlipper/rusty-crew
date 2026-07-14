export const CODEX_CONSUMED_RESPONSE_SCHEMAS: Readonly<Record<string, string>> =
  Object.freeze({
    initialize: "v1/InitializeResponse.json",
    "model/list": "v2/ModelListResponse.json",
    "collaborationMode/list": "v2/CollaborationModeListResponse.json",
    "thread/list": "v2/ThreadListResponse.json",
    "thread/read": "v2/ThreadReadResponse.json",
    "thread/archive": "v2/ThreadArchiveResponse.json",
    "thread/fork": "v2/ThreadForkResponse.json",
    "thread/delete": "v2/ThreadDeleteResponse.json",
    "thread/unarchive": "v2/ThreadUnarchiveResponse.json",
    "thread/loaded/list": "v2/ThreadLoadedListResponse.json",
    "thread/start": "v2/ThreadStartResponse.json",
    "thread/resume": "v2/ThreadResumeResponse.json",
    "thread/name/set": "v2/ThreadSetNameResponse.json",
    "thread/turns/list": "v2/ThreadTurnsListResponse.json",
    "thread/items/list": "v2/ThreadItemsListResponse.json",
    "turn/start": "v2/TurnStartResponse.json",
    "turn/steer": "v2/TurnSteerResponse.json",
    "turn/interrupt": "v2/TurnInterruptResponse.json",
    "thread/compact/start": "v2/ThreadCompactStartResponse.json",
    "thread/settings/update": "v2/ThreadSettingsUpdateResponse.json",
  });

export const CODEX_CONSUMED_SERVER_REQUEST_RESPONSE_SCHEMAS: Readonly<
  Record<string, string>
> = Object.freeze({
  "item/commandExecution/requestApproval":
    "CommandExecutionRequestApprovalResponse.json",
  "item/fileChange/requestApproval": "FileChangeRequestApprovalResponse.json",
  "item/tool/requestUserInput": "ToolRequestUserInputResponse.json",
  "mcpServer/elicitation/request": "McpServerElicitationRequestResponse.json",
  "item/permissions/requestApproval": "PermissionsRequestApprovalResponse.json",
  "item/tool/call": "DynamicToolCallResponse.json",
  "account/chatgptAuthTokens/refresh": "ChatgptAuthTokensRefreshResponse.json",
  "attestation/generate": "AttestationGenerateResponse.json",
  "currentTime/read": "CurrentTimeReadResponse.json",
  applyPatchApproval: "ApplyPatchApprovalResponse.json",
  execCommandApproval: "ExecCommandApprovalResponse.json",
});

export const CODEX_CONSUMED_INBOUND_SCHEMAS = Object.freeze({
  serverRequest: "ServerRequest.json",
  serverNotification: "ServerNotification.json",
});

export function allowAdditiveObjectFields(schema: unknown): object {
  return relaxSchemaNode(schema) as object;
}

function relaxSchemaNode(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(relaxSchemaNode);
  if (!isRecord(value)) return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(
        ([key, child]) => key !== "additionalProperties" || child !== false,
      )
      .map(([key, child]) => [key, relaxSchemaNode(child)]),
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
