import { createHash } from "node:crypto";
import type { BoundedRawDetail } from "./types.js";

const secretKey =
  /^(authorization|proxy-authorization|api[-_]?key|access[-_]?token|refresh[-_]?token|id[-_]?token|password|secret|cookie|set-cookie)$/i;

export function captureBoundedRawDetail(
  value: unknown,
  maxBytes: number,
): BoundedRawDetail {
  const original = JSON.stringify(value) ?? "null";
  const redactedKeys = new Set<string>();
  const sanitized =
    JSON.stringify(value, (key, child) => {
      if (key !== "" && secretKey.test(key)) {
        redactedKeys.add(key);
        return "[REDACTED]";
      }
      return child;
    }) ?? "null";
  const buffer = Buffer.from(sanitized, "utf8");
  const truncated = buffer.byteLength > maxBytes;
  const bounded = truncated
    ? buffer.subarray(0, maxBytes).toString("utf8")
    : sanitized;
  return {
    json: bounded,
    originalSha256: createHash("sha256").update(original).digest("hex"),
    truncated,
    redactedKeys: [...redactedKeys].sort(),
  };
}
