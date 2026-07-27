import type { NeutralExternalRuntimeEventPayload } from "./types.js";

export const CODEX_ERROR_DIAGNOSTIC_LIMITS = Object.freeze({
  message: 4_096,
  code: 256,
  additionalDetails: 8_192,
});

export type CodexErrorDiagnostic = Omit<
  NonNullable<NeutralExternalRuntimeEventPayload["error"]>,
  "willRetry"
>;

const TRUNCATION_MARKER = "...[truncated]";
const UNSAFE_CONTROL_CHARACTERS =
  /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g;

export function projectCodexErrorDiagnostic(
  value: unknown,
): CodexErrorDiagnostic | undefined {
  const error = recordValue(value);
  const message = boundedDiagnosticString(
    error.message,
    CODEX_ERROR_DIAGNOSTIC_LIMITS.message,
  );
  if (message === undefined || message.length === 0) return undefined;
  return {
    message,
    code: projectCodexErrorCode(error),
    additionalDetails:
      boundedDiagnosticString(
        error.additionalDetails,
        CODEX_ERROR_DIAGNOSTIC_LIMITS.additionalDetails,
      ) ?? null,
  };
}

function projectCodexErrorCode(
  error: Readonly<Record<string, unknown>>,
): string | null {
  const explicitCode = boundedDiagnosticString(
    error.code,
    CODEX_ERROR_DIAGNOSTIC_LIMITS.code,
  );
  if (explicitCode !== undefined && explicitCode.length > 0) {
    return explicitCode;
  }
  const info = error.codexErrorInfo;
  const nativeCode =
    typeof info === "string" ? info : Object.keys(recordValue(info))[0];
  const bounded = boundedDiagnosticString(
    nativeCode,
    CODEX_ERROR_DIAGNOSTIC_LIMITS.code,
  );
  return bounded === undefined || bounded.length === 0 ? null : bounded;
}

function boundedDiagnosticString(
  value: unknown,
  maxLength: number,
): string | undefined {
  if (typeof value !== "string") return undefined;
  const sanitized = value.replace(UNSAFE_CONTROL_CHARACTERS, " ");
  if (sanitized.length <= maxLength) return sanitized;
  const contentLength = Math.max(0, maxLength - TRUNCATION_MARKER.length);
  let end = contentLength;
  if (end > 0 && isHighSurrogate(sanitized.charCodeAt(end - 1))) {
    end -= 1;
  }
  return `${sanitized.slice(0, end)}${TRUNCATION_MARKER}`;
}

function recordValue(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function isHighSurrogate(codeUnit: number): boolean {
  return codeUnit >= 0xd800 && codeUnit <= 0xdbff;
}
