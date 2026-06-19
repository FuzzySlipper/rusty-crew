import type {
  BodyState,
  BrainWakeRequest,
  RuntimeBufferHandle,
  RuntimeBufferView,
  Unit,
} from "@rusty-crew/contracts";
import type {
  BrainImplementation,
  BrainRoleAssembly,
  BrainWakeResult,
} from "./index.js";

export interface BridgeBufferClient {
  getBuffer(handle: RuntimeBufferHandle): Promise<RuntimeBufferView>;
  releaseBuffer(handle: RuntimeBufferHandle): Promise<Unit>;
}

export async function wakeBrainFromBridgeRequest(
  buffers: BridgeBufferClient,
  brain: BrainImplementation,
  request: BrainWakeRequest,
): Promise<BrainWakeResult> {
  const handles = [
    request.bodyState,
    request.systemPrompt,
    request.roleAssembly,
  ];
  let wakeFailed = false;

  try {
    const [bodyStateView, systemPromptView, roleAssemblyView] =
      await Promise.all([
        buffers.getBuffer(request.bodyState),
        buffers.getBuffer(request.systemPrompt),
        buffers.getBuffer(request.roleAssembly),
      ]);

    return await brain.wake({
      wakeId: request.wakeId,
      sessionId: request.sessionId,
      state: parseJsonBuffer<BodyState>(bodyStateView),
      systemPrompt: decodeBuffer(systemPromptView),
      roleAssembly: parseJsonBuffer<BrainRoleAssembly>(roleAssemblyView),
    });
  } catch (error) {
    wakeFailed = true;
    throw error;
  } finally {
    const releases = await Promise.allSettled(
      handles.map((handle) => buffers.releaseBuffer(handle)),
    );
    const failedRelease = releases.find(
      (result): result is PromiseRejectedResult => result.status === "rejected",
    );

    if (!wakeFailed && failedRelease) {
      throw failedRelease.reason;
    }
  }
}

function parseJsonBuffer<T>(view: RuntimeBufferView): T {
  return JSON.parse(decodeBuffer(view)) as T;
}

function decodeBuffer(view: RuntimeBufferView): string {
  return new TextDecoder().decode(view.bytes);
}
