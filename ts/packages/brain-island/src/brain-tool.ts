import type { Static, TSchema } from "typebox";
import type { BrainWakeInput } from "./index.js";

export type BrainToolExecutionMode = "sequential" | "parallel";

export type BrainToolContent =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string };

export interface BrainToolResult<TDetails = unknown> {
  content: BrainToolContent[];
  details: TDetails;
  terminate?: boolean;
}

export type BrainToolUpdateCallback<TDetails = unknown> = (
  partialResult: BrainToolResult<TDetails>,
) => void;

export interface BrainToolContext<TDetails = unknown> {
  wake: BrainWakeInput;
  wakeId: string;
  sessionId: string;
  callId: string;
  signal: AbortSignal;
  onUpdate?: BrainToolUpdateCallback<TDetails>;
}

export interface BrainTool<
  TParameters extends TSchema = TSchema,
  TDetails = unknown,
> {
  name: string;
  description: string;
  label: string;
  parameters: TParameters;
  prepareArguments?: (args: unknown) => Static<TParameters>;
  execute(
    callId: string,
    params: Static<TParameters>,
    signal?: AbortSignal,
    onUpdate?: BrainToolUpdateCallback<TDetails>,
  ): Promise<BrainToolResult<TDetails>>;
  executeWithContext?(
    params: Static<TParameters>,
    context: BrainToolContext<TDetails>,
  ): Promise<BrainToolResult<TDetails>>;
  executionMode?: BrainToolExecutionMode;
}
