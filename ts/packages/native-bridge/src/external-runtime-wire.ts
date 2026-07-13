import type {
  DenRuntimeReference,
  ExternalAgentBindingMetadataWrite,
  ExternalAgentSessionCreationRequest,
} from "@rusty-crew/contracts";

export function serializeExternalAgentSessionCreationRequest(
  request: ExternalAgentSessionCreationRequest,
): string {
  return JSON.stringify({
    ...request,
    ...(request.taskRef == null
      ? {}
      : { taskRef: serializeDenRuntimeReference(request.taskRef) }),
  });
}

export function serializeExternalAgentBindingMetadataWrite(
  write: ExternalAgentBindingMetadataWrite,
): string {
  return JSON.stringify({
    ...write,
    ...(write.taskRef == null
      ? { taskRef: null }
      : { taskRef: serializeDenRuntimeReference(write.taskRef) }),
  });
}

function serializeDenRuntimeReference(
  reference: DenRuntimeReference,
): Record<string, string> {
  const raw = reference as DenRuntimeReference & {
    project_id?: string;
    task_id?: string;
  };
  const projectId = reference.projectId ?? raw.project_id;
  const taskId = reference.taskId ?? raw.task_id;
  return {
    ...(projectId === undefined ? {} : { project_id: projectId }),
    ...(taskId === undefined ? {} : { task_id: taskId }),
  };
}
