export const NATIVE_REASONING_EFFORTS = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
] as const;

export type NativeReasoningEffort = (typeof NATIVE_REASONING_EFFORTS)[number];

/**
 * Describes where the effective native-brain effort came from. A configured
 * provider value is reported as `profile` because the active profile selects
 * that provider; `provider_default` means no profile-selected value exists.
 */
export type ReasoningEffortSource =
  | "session_override"
  | "profile"
  | "provider_default";

export interface ResolvedReasoningEffort {
  value?: string;
  source: ReasoningEffortSource;
}

const NATIVE_REASONING_EFFORT_SET = new Set<string>(NATIVE_REASONING_EFFORTS);

export function isNativeReasoningEffort(
  value: unknown,
): value is NativeReasoningEffort {
  return typeof value === "string" && NATIVE_REASONING_EFFORT_SET.has(value);
}

export function nativeReasoningEffortList(): string {
  return NATIVE_REASONING_EFFORTS.join(", ");
}

export function resolveReasoningEffort(
  sessionOverride: string | undefined,
  profileReasoningEffort: string | undefined,
): ResolvedReasoningEffort {
  if (sessionOverride !== undefined) {
    return { value: sessionOverride, source: "session_override" };
  }
  if (profileReasoningEffort !== undefined) {
    return { value: profileReasoningEffort, source: "profile" };
  }
  return { source: "provider_default" };
}
