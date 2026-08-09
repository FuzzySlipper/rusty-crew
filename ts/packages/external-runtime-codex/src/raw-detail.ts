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
    JSON.stringify(value, function (key, child) {
      if (key !== "" && secretKey.test(key)) {
        redactedKeys.add(key);
        return "[REDACTED]";
      }
      if (
        typeof child === "string" &&
        ((key === "imageUrl" &&
          typeof this === "object" &&
          this !== null &&
          ["inputImage", "input_image"].includes(
            String((this as Record<string, unknown>).type),
          )) ||
          (key === "data" &&
            typeof this === "object" &&
            this !== null &&
            (this as Record<string, unknown>).type === "image"))
      ) {
        redactedKeys.add(key);
        return `[MEDIA_BYTES_REDACTED sha256=${createHash("sha256").update(child).digest("hex")}]`;
      }
      if (
        key === "path" &&
        typeof child === "string" &&
        typeof this === "object" &&
        this !== null &&
        ["imageView", "image_view"].includes(
          String((this as Record<string, unknown>).type),
        )
      ) {
        redactedKeys.add(key);
        return "[HOST_MEDIA_PATH_REDACTED]";
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
