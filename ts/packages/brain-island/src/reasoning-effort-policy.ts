export const NATIVE_REASONING_EFFORTS = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
] as const;

export type NativeReasoningEffort = (typeof NATIVE_REASONING_EFFORTS)[number];

const NATIVE_REASONING_EFFORT_SET = new Set<string>(NATIVE_REASONING_EFFORTS);

export function isNativeReasoningEffort(
  value: unknown,
): value is NativeReasoningEffort {
  return typeof value === "string" && NATIVE_REASONING_EFFORT_SET.has(value);
}

export function nativeReasoningEffortList(): string {
  return NATIVE_REASONING_EFFORTS.join(", ");
}
