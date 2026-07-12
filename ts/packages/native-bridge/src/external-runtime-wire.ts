import type { ExternalAgentSessionCreationRequest } from "@rusty-crew/contracts";

export function serializeExternalAgentSessionCreationRequest(
  request: ExternalAgentSessionCreationRequest,
): string {
  return JSON.stringify({
    ...request,
    ...(request.taskRef == null
      ? {}
      : {
          taskRef: {
            ...(request.taskRef.projectId === undefined
              ? {}
              : { project_id: request.taskRef.projectId }),
            ...(request.taskRef.taskId === undefined
              ? {}
              : { task_id: request.taskRef.taskId }),
          },
        }),
  });
}
