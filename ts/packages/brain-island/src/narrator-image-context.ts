import type { SessionId } from "@rusty-crew/contracts";

export const MAX_NARRATOR_CONTEXT_IMAGES = 4;
export const MAX_NARRATOR_CONTEXT_IMAGE_BYTES = 10 * 1024 * 1024;
export const MAX_NARRATOR_CONTEXT_TOTAL_BYTES = 20 * 1024 * 1024;

export interface NarratorImageInputCapability {
  supported: boolean;
  maxImages: number;
  maxImageBytes: number;
  maxTotalBytes: number;
  reasonCode?: string;
}

export interface NarratorImageInput {
  attachmentId: string;
  mimeType: string;
  bytesBase64: string;
  byteSize: number;
}

export interface NarratorImageContextDiagnostic {
  reasonCode: string;
  attachmentId?: string;
  summary: string;
}

export interface NarratorImageContextResolution {
  capability: NarratorImageInputCapability;
  selectedAttachmentIds: string[];
  images: NarratorImageInput[];
  diagnostics: NarratorImageContextDiagnostic[];
}

export interface NarratorImageContextResolver {
  resolveNarratorImageContext(input: {
    sessionId: SessionId;
    capability: NarratorImageInputCapability;
  }): Promise<NarratorImageContextResolution>;
}

export function narratorImageInputCapability(
  metadataJson: unknown,
): NarratorImageInputCapability {
  const metadata = recordValue(metadataJson);
  const configured = recordValue(
    metadata.narrator_image_input ?? metadata.narratorImageInput,
  );
  if (configured.supported !== true) {
    return {
      supported: false,
      maxImages: 0,
      maxImageBytes: 0,
      maxTotalBytes: 0,
      reasonCode: "narrator_image_input_not_configured",
    };
  }
  return {
    supported: true,
    maxImages: boundedPositiveInteger(
      configured.max_images ?? configured.maxImages,
      MAX_NARRATOR_CONTEXT_IMAGES,
      MAX_NARRATOR_CONTEXT_IMAGES,
    ),
    maxImageBytes: boundedPositiveInteger(
      configured.max_image_bytes ?? configured.maxImageBytes,
      MAX_NARRATOR_CONTEXT_IMAGE_BYTES,
      MAX_NARRATOR_CONTEXT_IMAGE_BYTES,
    ),
    maxTotalBytes: boundedPositiveInteger(
      configured.max_total_bytes ?? configured.maxTotalBytes,
      MAX_NARRATOR_CONTEXT_TOTAL_BYTES,
      MAX_NARRATOR_CONTEXT_TOTAL_BYTES,
    ),
  };
}

function boundedPositiveInteger(
  value: unknown,
  fallback: number,
  ceiling: number,
): number {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? Math.min(value, ceiling)
    : fallback;
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
