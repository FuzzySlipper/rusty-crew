import { createHash } from "node:crypto";
import { mkdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import type { SessionId } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import type { BrainToolResult } from "./brain-tool.js";
import type {
  BrainToolMediaReference,
  BrainToolMediaSink,
} from "./brain-tool-media.js";
import type { AttachmentRecord, ChatEvent } from "./rusty-view-chat-api.js";
import type {
  NarratorImageContextResolution,
  NarratorImageInputCapability,
} from "./narrator-image-context.js";
import type {
  ExternalRuntimeMediaCaptureInput,
  ExternalRuntimeMediaCaptureSink,
  ExternalRuntimeMediaReference,
} from "./external-runtime-media.js";

export const MAX_CHAT_IMAGE_BYTES = 20 * 1024 * 1024;
const MIME_EXTENSIONS = new Map([
  ["image/png", "png"],
  ["image/jpeg", "jpg"],
  ["image/gif", "gif"],
  ["image/webp", "webp"],
]);

export interface ToolMediaAttachmentStoreOptions {
  artifactDir: string;
  bridge: Pick<
    NativeBridgeModule,
    "createChatAttachment" | "queryAttachmentsPage" | "removeChatAttachment"
  >;
  appendChatEvent(
    sessionId: SessionId,
    event: Pick<ChatEvent, "kind" | "payload">,
  ): Promise<ChatEvent>;
  now(): string;
  maxImageBytes?: number;
}

export interface ToolMediaAttachmentContent {
  attachment: AttachmentRecord;
  bytes: Buffer;
}

export class ToolMediaAttachmentStore
  implements BrainToolMediaSink, ExternalRuntimeMediaCaptureSink
{
  private readonly rootDir: string;
  private readonly maxImageBytes: number;

  constructor(private readonly options: ToolMediaAttachmentStoreOptions) {
    this.rootDir = join(options.artifactDir, "tool-media");
    this.maxImageBytes = options.maxImageBytes ?? MAX_CHAT_IMAGE_BYTES;
  }

  async persistImages(input: {
    sessionId: string;
    wakeId: string;
    callId: string;
    toolName: string;
    result: BrainToolResult;
  }): Promise<readonly BrainToolMediaReference[]> {
    const images = input.result.content.filter(
      (
        item,
      ): item is Extract<
        (typeof input.result.content)[number],
        { type: "image" }
      > => item.type === "image",
    );
    const references: BrainToolMediaReference[] = [];
    for (const [index, image] of images.entries()) {
      references.push(
        await this.persistImage({
          ...input,
          image,
          imageIndex: index,
        }),
      );
    }
    return references;
  }

  async captureExternalRuntimeMedia(
    input: ExternalRuntimeMediaCaptureInput,
  ): Promise<readonly ExternalRuntimeMediaReference[]> {
    if (input.sessionId === undefined) {
      return input.candidates.map((candidate) => ({
        mediaIndex: candidate.mediaIndex,
        captureSource: candidate.source,
        captureState: "unavailable",
        reasonCode: "external_media_session_unbound",
      }));
    }

    const references: ExternalRuntimeMediaReference[] = [];
    const capturedSourceByHash = new Map<
      string,
      ExternalRuntimeMediaCaptureInput["candidates"][number]["source"]
    >();
    for (const candidate of input.candidates) {
      try {
        const materialized = await materializeExternalMedia(candidate);
        if (materialized.bytes.length === 0) {
          references.push({
            mediaIndex: candidate.mediaIndex,
            captureSource: candidate.source,
            captureState: "empty",
            reasonCode: "external_media_empty",
          });
          continue;
        }
        if (materialized.bytes.length > this.maxImageBytes) {
          references.push({
            mediaIndex: candidate.mediaIndex,
            captureSource: candidate.source,
            captureState: "oversized",
            reasonCode: "external_media_oversized",
            mimeType: materialized.mimeType,
            byteSize: materialized.bytes.length,
          });
          continue;
        }
        const sha256 = createHash("sha256")
          .update(materialized.bytes)
          .digest("hex");
        const capturedSource = capturedSourceByHash.get(sha256);
        if (
          capturedSource !== undefined &&
          capturedSource !== candidate.source
        ) {
          continue;
        }
        capturedSourceByHash.set(sha256, candidate.source);
        const stored = await this.persistStoredImage({
          sessionId: input.sessionId,
          identity: [
            input.runtimeId,
            input.bindingId ?? input.sessionId,
            input.nativeThreadId ?? "thread-unknown",
            input.nativeTurnId ?? "turn-unknown",
            input.itemId ?? input.externalEventId,
            String(candidate.mediaIndex),
          ].join(":"),
          filename: materialized.filename,
          mimeType: materialized.mimeType,
          bytes: materialized.bytes,
          metadata: {
            source: "external_runtime_media",
            runtime_id: input.runtimeId,
            binding_id: input.bindingId ?? null,
            native_thread_id: input.nativeThreadId ?? null,
            native_turn_id: input.nativeTurnId ?? null,
            item_id: input.itemId ?? null,
            external_event_id: input.externalEventId,
            media_index: candidate.mediaIndex,
            capture_source: candidate.source,
            tool_name: input.toolName ?? null,
          },
        });
        references.push({
          mediaIndex: candidate.mediaIndex,
          captureSource: candidate.source,
          captureState: "available",
          attachmentId: stored.attachmentId,
          filename: stored.filename,
          mimeType: stored.mimeType,
          byteSize: stored.byteSize,
          sha256,
          width: stored.width,
          height: stored.height,
          contentUrl: stored.downloadUrl,
        });
      } catch (error) {
        references.push(externalCaptureFailure(candidate, error));
      }
    }
    return references;
  }

  async attachmentsForWake(
    sessionId: string,
    wakeId: string,
  ): Promise<AttachmentRecord[]> {
    const records = await this.queryAllAttachments({
      session_id: sessionId,
      status: "active",
      include_removed: false,
      include_expired: false,
      expired_only: false,
      now: this.options.now(),
    });
    return records.filter((record) => {
      const metadata = recordValue(record.metadata_json);
      return (
        metadata.source === "brain_tool_media" && metadata.wake_id === wakeId
      );
    });
  }

  async persistUploadedImage(input: {
    sessionId: string;
    idempotencyKey: string;
    filename: string;
    mimeType: string;
    bytes: Buffer;
  }): Promise<ToolMediaAttachmentContent> {
    const stored = await this.persistStoredImage({
      sessionId: input.sessionId,
      identity: `${input.sessionId}:chat-upload:${input.idempotencyKey}`,
      filename: input.filename,
      mimeType: input.mimeType,
      bytes: input.bytes,
      metadata: {
        source: "chat_upload",
        idempotency_key: input.idempotencyKey,
      },
    });
    return this.readContent(input.sessionId, stored.attachmentId);
  }

  async uploadedAttachments(
    sessionId: string,
    attachmentIds: readonly string[],
  ): Promise<AttachmentRecord[]> {
    const attachments: AttachmentRecord[] = [];
    for (const attachmentId of attachmentIds) {
      const attachment = await this.findAttachment(sessionId, attachmentId);
      if (attachment.status !== "active") {
        throw new ToolMediaAttachmentError(
          "attachment_removed",
          `attachment ${attachmentId} is removed`,
        );
      }
      attachments.push(attachment);
    }
    return attachments;
  }

  async resolveNarratorImageContext(input: {
    sessionId: SessionId;
    capability: NarratorImageInputCapability;
  }): Promise<NarratorImageContextResolution> {
    const records = await this.queryAllAttachments({
      session_id: input.sessionId,
      include_removed: true,
      include_expired: true,
      expired_only: false,
      now: this.options.now(),
    });
    const selected = records
      .flatMap((attachment) =>
        attachment.links
          .filter((link) => {
            const metadata = recordValue(link.metadata_json);
            return (
              metadata.source === "roleplay_image_generation" &&
              metadata.include_in_narrator_context === true
            );
          })
          .map((link) => ({ attachment, link })),
      )
      .sort(
        (left, right) =>
          left.link.created_at.localeCompare(right.link.created_at) ||
          left.attachment.attachment_id.localeCompare(
            right.attachment.attachment_id,
          ) ||
          left.link.link_id.localeCompare(right.link.link_id),
      );
    const selectedAttachmentIds = [
      ...new Set(selected.map(({ attachment }) => attachment.attachment_id)),
    ];
    const diagnostics: NarratorImageContextResolution["diagnostics"] = [];
    if (!input.capability.supported) {
      if (selectedAttachmentIds.length > 0) {
        diagnostics.push({
          reasonCode:
            input.capability.reasonCode ?? "narrator_image_input_unsupported",
          summary: `${selectedAttachmentIds.length} opted-in narrator image(s) were omitted because the provider is not configured for image input`,
        });
      }
      return {
        capability: input.capability,
        selectedAttachmentIds,
        images: [],
        diagnostics,
      };
    }

    const images: NarratorImageContextResolution["images"] = [];
    let totalBytes = 0;
    for (const attachmentId of selectedAttachmentIds) {
      const attachment = records.find(
        (candidate) => candidate.attachment_id === attachmentId,
      );
      if (!attachment) {
        diagnostics.push({
          reasonCode: "narrator_image_attachment_missing",
          attachmentId,
          summary: "An opted-in narrator image attachment was not found",
        });
        continue;
      }
      if (attachment.status !== "active") {
        diagnostics.push({
          reasonCode: "narrator_image_attachment_removed",
          attachmentId,
          summary: "An opted-in narrator image attachment has been removed",
        });
        continue;
      }
      if (images.length >= input.capability.maxImages) {
        diagnostics.push({
          reasonCode: "narrator_image_count_limit",
          attachmentId,
          summary: `Narrator image input is limited to ${input.capability.maxImages} image(s)`,
        });
        continue;
      }
      if (attachment.byte_size > input.capability.maxImageBytes) {
        diagnostics.push({
          reasonCode: "narrator_image_size_limit",
          attachmentId,
          summary: `Narrator image ${attachmentId} exceeds the configured per-image byte limit`,
        });
        continue;
      }
      if (totalBytes + attachment.byte_size > input.capability.maxTotalBytes) {
        diagnostics.push({
          reasonCode: "narrator_image_total_size_limit",
          attachmentId,
          summary:
            "Narrator image input exceeds the configured total byte limit",
        });
        continue;
      }
      try {
        const content = await this.readContent(input.sessionId, attachmentId);
        if (content.bytes.length !== attachment.byte_size) {
          throw new ToolMediaAttachmentError(
            "attachment_byte_size_mismatch",
            "attachment byte size does not match stored content",
          );
        }
        images.push({
          attachmentId,
          mimeType: attachment.mime_type,
          bytesBase64: content.bytes.toString("base64"),
          byteSize: content.bytes.length,
        });
        totalBytes += content.bytes.length;
      } catch (error) {
        diagnostics.push({
          reasonCode:
            error instanceof ToolMediaAttachmentError
              ? error.reasonCode
              : "narrator_image_content_unavailable",
          attachmentId,
          summary:
            error instanceof Error
              ? error.message
              : "Narrator image content is unavailable",
        });
      }
    }
    return {
      capability: input.capability,
      selectedAttachmentIds,
      images,
      diagnostics,
    };
  }

  async linkAttachmentsToMessage(input: {
    sessionId: string;
    wakeId: string;
    messageId: string;
    blockIdsByAttachmentId: ReadonlyMap<string, string>;
  }): Promise<void> {
    const attachments = await this.attachmentsForWake(
      input.sessionId,
      input.wakeId,
    );
    await this.linkAttachmentRecordsToMessage({
      sessionId: input.sessionId,
      messageId: input.messageId,
      attachments,
      blockIdsByAttachmentId: input.blockIdsByAttachmentId,
      metadata: {
        source: "brain_tool_media",
        wake_id: input.wakeId,
      },
    });
  }

  async linkUploadedAttachmentsToMessage(input: {
    sessionId: string;
    attachmentIds: readonly string[];
    messageId: string;
    blockIdsByAttachmentId: ReadonlyMap<string, string>;
  }): Promise<AttachmentRecord[]> {
    const attachments = await this.uploadedAttachments(
      input.sessionId,
      input.attachmentIds,
    );
    await this.linkAttachmentRecordsToMessage({
      sessionId: input.sessionId,
      messageId: input.messageId,
      attachments,
      blockIdsByAttachmentId: input.blockIdsByAttachmentId,
      metadata: { source: "chat_upload" },
    });
    return attachments;
  }

  private async linkAttachmentRecordsToMessage(input: {
    sessionId: string;
    messageId: string;
    attachments: readonly AttachmentRecord[];
    blockIdsByAttachmentId: ReadonlyMap<string, string>;
    metadata: Record<string, unknown>;
  }): Promise<void> {
    for (const attachment of input.attachments) {
      const blockId = input.blockIdsByAttachmentId.get(
        attachment.attachment_id,
      );
      if (!blockId) continue;
      const now = this.options.now();
      const link = {
        link_id: stableId(
          "attachment-link",
          `${attachment.attachment_id}:${input.messageId}:${blockId}`,
        ),
        attachment_id: attachment.attachment_id,
        session_id: input.sessionId,
        message_id: input.messageId,
        block_id: blockId,
        scope_id: null,
        metadata_json: input.metadata,
        created_at: now,
      };
      const result = (await this.options.bridge.createChatAttachment({
        attachment: {
          ...attachment,
          status: "active",
          created_at: attachment.created_at,
          updated_at: now,
          link,
          links: undefined,
        },
      })) as { attachment: AttachmentRecord };
      await this.options.appendChatEvent(input.sessionId as SessionId, {
        kind: "attachment_linked",
        payload: {
          attachment_id: attachment.attachment_id,
          link,
          attachment: result.attachment,
        },
      });
    }
  }

  async readContent(
    sessionId: string,
    attachmentId: string,
  ): Promise<ToolMediaAttachmentContent> {
    const attachment = await this.findAttachment(sessionId, attachmentId);
    if (attachment.status !== "active") {
      throw new ToolMediaAttachmentError(
        "attachment_removed",
        "attachment is removed",
      );
    }
    if (
      attachment.expires_at &&
      Date.parse(attachment.expires_at) <= Date.parse(this.options.now())
    ) {
      await this.removeStoredBytes(attachment).catch(() => undefined);
      throw new ToolMediaAttachmentError(
        "attachment_expired",
        "attachment has expired",
      );
    }
    const path = this.pathFromAttachment(attachment);
    return { attachment, bytes: await readFile(path) };
  }

  async removeContent(attachment: AttachmentRecord): Promise<void> {
    if (!isToolMediaSource(recordValue(attachment.metadata_json).source)) {
      return;
    }
    await this.removeStoredBytes(attachment);
  }

  private async persistImage(input: {
    sessionId: string;
    wakeId: string;
    callId: string;
    toolName: string;
    result: BrainToolResult;
    image: { type: "image"; data: string; mimeType: string };
    imageIndex: number;
  }): Promise<BrainToolMediaReference> {
    const bytes = decodeBase64(input.image.data);
    return this.persistStoredImage({
      sessionId: input.sessionId,
      identity: `${input.sessionId}:${input.wakeId}:${input.callId}:${input.imageIndex}`,
      filename: `${safeStem(input.toolName)}-${input.imageIndex + 1}.${MIME_EXTENSIONS.get(input.image.mimeType) ?? "image"}`,
      mimeType: input.image.mimeType,
      bytes,
      metadata: {
        source: "brain_tool_media",
        wake_id: input.wakeId,
        tool_call_id: input.callId,
        tool_name: input.toolName,
        image_index: input.imageIndex,
        provenance: safeProvenance(input.result.details),
      },
    });
  }

  private async persistStoredImage(input: {
    sessionId: string;
    identity: string;
    filename: string;
    mimeType: string;
    bytes: Buffer;
    metadata: Record<string, unknown>;
  }): Promise<BrainToolMediaReference> {
    const extension = MIME_EXTENSIONS.get(input.mimeType);
    if (!extension) {
      throw new ToolMediaAttachmentError(
        "unsupported_image_mime_type",
        `unsupported tool image MIME type ${input.mimeType}`,
      );
    }
    if (input.bytes.length === 0 || input.bytes.length > this.maxImageBytes) {
      throw new ToolMediaAttachmentError(
        "invalid_image_byte_size",
        `tool image byte size ${input.bytes.length} is outside 1..${this.maxImageBytes}`,
      );
    }
    const dimensions = imageDimensions(input.bytes, input.mimeType);
    const attachmentId = stableId("attachment", input.identity);
    const filename = safeDisplayFilename(input.filename, extension);
    const sessionDir = join(this.rootDir, digest(input.sessionId));
    const finalPath = join(sessionDir, `${digest(attachmentId)}.${extension}`);
    const temporaryPath = `${finalPath}.tmp-${process.pid}-${Date.now()}`;
    const now = this.options.now();
    const downloadUrl = toolMediaDownloadUrl(input.sessionId, attachmentId);
    const contentSha256 = createHash("sha256")
      .update(input.bytes)
      .digest("hex");
    const existing = await this.findAttachmentOptional(
      input.sessionId,
      attachmentId,
    );
    if (existing !== undefined) {
      const metadata = recordValue(existing.metadata_json);
      if (
        existing.status !== "active" ||
        existing.mime_type !== input.mimeType ||
        existing.byte_size !== input.bytes.length ||
        metadata.content_sha256 !== contentSha256
      ) {
        throw new ToolMediaAttachmentError(
          "attachment_idempotency_conflict",
          `tool media attachment ${attachmentId} conflicts with an existing record`,
        );
      }
      await readFile(this.pathFromAttachment(existing));
      return mediaReference(existing);
    }
    await mkdir(sessionDir, { recursive: true });
    await writeFile(temporaryPath, input.bytes, { flag: "wx", mode: 0o600 });
    await rename(temporaryPath, finalPath).catch(async (error) => {
      await rm(temporaryPath, { force: true }).catch(() => undefined);
      throw error;
    });
    try {
      const result = (await this.options.bridge.createChatAttachment({
        attachment: {
          attachment_id: attachmentId,
          session_id: input.sessionId,
          status: "active",
          filename,
          mime_type: input.mimeType,
          byte_size: input.bytes.length,
          storage_url: `artifact://tool-media/${digest(input.sessionId)}/${basename(finalPath)}`,
          download_url: downloadUrl,
          thumbnail_url: null,
          extracted_text: null,
          extracted_text_truncated: false,
          metadata_json: {
            ...input.metadata,
            width: dimensions.width,
            height: dimensions.height,
            content_sha256: contentSha256,
          },
          created_at: now,
          updated_at: now,
          expires_at: null,
          link: null,
        },
      })) as { attachment: AttachmentRecord };
      await this.options.appendChatEvent(input.sessionId as SessionId, {
        kind: "attachment_uploaded",
        payload: { attachment: result.attachment },
      });
      return {
        attachmentId,
        filename,
        mimeType: input.mimeType,
        byteSize: input.bytes.length,
        width: dimensions.width,
        height: dimensions.height,
        downloadUrl,
      };
    } catch (error) {
      await rm(finalPath, { force: true }).catch(() => undefined);
      await this.options.bridge
        .removeChatAttachment({
          session_id: input.sessionId,
          attachment_id: attachmentId,
          updated_at: this.options.now(),
        })
        .catch(() => undefined);
      throw error;
    }
  }

  private async findAttachment(
    sessionId: string,
    attachmentId: string,
  ): Promise<AttachmentRecord> {
    const attachment = await this.findAttachmentOptional(
      sessionId,
      attachmentId,
    );
    if (!attachment) {
      throw new ToolMediaAttachmentError(
        "attachment_not_found",
        "attachment was not found",
      );
    }
    return attachment;
  }

  private async findAttachmentOptional(
    sessionId: string,
    attachmentId: string,
  ): Promise<AttachmentRecord | undefined> {
    const records = await this.queryAllAttachments({
      session_id: sessionId,
      include_removed: true,
      include_expired: true,
      expired_only: false,
      now: this.options.now(),
    });
    const attachment = records.find(
      (candidate) => candidate.attachment_id === attachmentId,
    );
    if (!attachment) return undefined;
    const metadata = recordValue(attachment.metadata_json);
    if (!isToolMediaSource(metadata.source)) {
      throw new ToolMediaAttachmentError(
        "attachment_content_unavailable",
        "attachment does not use Crew-owned tool media storage",
      );
    }
    return attachment;
  }

  private async queryAllAttachments(
    query: Record<string, unknown>,
  ): Promise<AttachmentRecord[]> {
    const records: AttachmentRecord[] = [];
    let offset = 0;
    for (;;) {
      const page = (await this.options.bridge.queryAttachmentsPage({
        ...query,
        page: { limit: 1_000, offset },
      })) as {
        items: AttachmentRecord[];
        total: number;
        next_offset?: number | null;
      };
      records.push(...page.items);
      if (page.next_offset === undefined || page.next_offset === null) break;
      if (page.next_offset <= offset) {
        throw new ToolMediaAttachmentError(
          "attachment_page_cursor_invalid",
          "attachment query returned a non-advancing page cursor",
        );
      }
      offset = page.next_offset;
    }
    return records;
  }

  private pathFromAttachment(attachment: AttachmentRecord): string {
    const extension = MIME_EXTENSIONS.get(attachment.mime_type);
    if (!extension) {
      throw new ToolMediaAttachmentError(
        "attachment_content_unavailable",
        "attachment MIME type is not stored by the tool media store",
      );
    }
    return join(
      this.rootDir,
      digest(attachment.session_id),
      `${digest(attachment.attachment_id)}.${extension}`,
    );
  }

  private async removeStoredBytes(attachment: AttachmentRecord): Promise<void> {
    await rm(this.pathFromAttachment(attachment), { force: true });
  }
}

export class ToolMediaAttachmentError extends Error {
  constructor(
    readonly reasonCode: string,
    message: string,
  ) {
    super(message);
    this.name = "ToolMediaAttachmentError";
  }
}

export function toolMediaDownloadUrl(
  sessionId: string,
  attachmentId: string,
): string {
  return `/v1/chat/sessions/${encodeURIComponent(sessionId)}/attachments/${encodeURIComponent(attachmentId)}/content`;
}

function decodeBase64(value: string): Buffer {
  const compact = value.replace(/\s+/g, "");
  if (
    !compact ||
    !/^[A-Za-z0-9+/]*={0,2}$/.test(compact) ||
    compact.length % 4 !== 0
  ) {
    throw new ToolMediaAttachmentError(
      "invalid_image_base64",
      "tool image data is not valid base64",
    );
  }
  const bytes = Buffer.from(compact, "base64");
  if (bytes.toString("base64") !== compact) {
    throw new ToolMediaAttachmentError(
      "invalid_image_base64",
      "tool image data is not canonical base64",
    );
  }
  return bytes;
}

function imageDimensions(
  bytes: Buffer,
  mimeType: string,
): { width: number; height: number } {
  let width = 0;
  let height = 0;
  if (
    mimeType === "image/png" &&
    bytes.length >= 24 &&
    bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))
  ) {
    width = bytes.readUInt32BE(16);
    height = bytes.readUInt32BE(20);
  } else if (
    mimeType === "image/gif" &&
    bytes.length >= 10 &&
    ["GIF87a", "GIF89a"].includes(bytes.subarray(0, 6).toString())
  ) {
    width = bytes.readUInt16LE(6);
    height = bytes.readUInt16LE(8);
  } else if (mimeType === "image/webp") {
    ({ width, height } = webpDimensions(bytes));
  } else if (mimeType === "image/jpeg") {
    ({ width, height } = jpegDimensions(bytes));
  }
  if (width < 1 || height < 1 || width > 100_000 || height > 100_000) {
    throw new ToolMediaAttachmentError(
      "invalid_image_dimensions",
      `tool image has invalid or unsupported ${mimeType} dimensions`,
    );
  }
  return { width, height };
}

function jpegDimensions(bytes: Buffer): { width: number; height: number } {
  if (bytes.length < 4 || bytes[0] !== 0xff || bytes[1] !== 0xd8)
    return { width: 0, height: 0 };
  let offset = 2;
  while (offset + 8 < bytes.length) {
    if (bytes[offset] !== 0xff) {
      offset += 1;
      continue;
    }
    const marker = bytes[offset + 1] ?? 0;
    offset += 2;
    if (marker === 0xd8 || marker === 0xd9) continue;
    const length = bytes.readUInt16BE(offset);
    if (length < 2 || offset + length > bytes.length) break;
    if (
      [
        0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce,
        0xcf,
      ].includes(marker)
    ) {
      return {
        height: bytes.readUInt16BE(offset + 3),
        width: bytes.readUInt16BE(offset + 5),
      };
    }
    offset += length;
  }
  return { width: 0, height: 0 };
}

function webpDimensions(bytes: Buffer): { width: number; height: number } {
  if (
    bytes.length < 30 ||
    bytes.subarray(0, 4).toString() !== "RIFF" ||
    bytes.subarray(8, 12).toString() !== "WEBP"
  )
    return { width: 0, height: 0 };
  const kind = bytes.subarray(12, 16).toString();
  if (kind === "VP8X") {
    return {
      width: 1 + bytes.readUIntLE(24, 3),
      height: 1 + bytes.readUIntLE(27, 3),
    };
  }
  return { width: 0, height: 0 };
}

function safeProvenance(details: unknown): Record<string, unknown> {
  const detailsRecord = recordValue(details);
  return recordValue(detailsRecord.provenance);
}

function recordValue(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function stableId(prefix: string, value: string): string {
  return `${prefix}:${digest(value)}`;
}

function digest(value: string): string {
  return createHash("sha256").update(value).digest("hex").slice(0, 32);
}

function safeStem(value: string): string {
  const stem = value
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return (stem || "tool-image").slice(0, 80);
}

function safeDisplayFilename(value: string, extension: string): string {
  const base = basename(value)
    .replace(/[\u0000-\u001f\u007f]/g, "")
    .replace(/[^A-Za-z0-9._ -]+/g, "-")
    .replace(/^\.+/, "")
    .trim()
    .slice(0, 120);
  if (!base) return `external-media.${extension}`;
  return extname(base) ? base : `${base}.${extension}`;
}

async function materializeExternalMedia(
  candidate: ExternalRuntimeMediaCaptureInput["candidates"][number],
): Promise<{ bytes: Buffer; mimeType: string; filename: string }> {
  if (candidate.source === "dynamic_tool_input_image") {
    const match = /^data:(image\/[A-Za-z0-9.+-]+);base64,([\s\S]*)$/.exec(
      candidate.imageUrl,
    );
    if (!match) {
      throw new ToolMediaAttachmentError(
        "external_media_data_url_unsupported",
        "dynamic tool image must use a base64 image data URL",
      );
    }
    const mimeType = match[1] ?? "";
    const extension = MIME_EXTENSIONS.get(mimeType) ?? "image";
    return {
      bytes: decodeExternalBase64(match[2] ?? ""),
      mimeType,
      filename: `dynamic-tool-image-${candidate.mediaIndex + 1}.${extension}`,
    };
  }
  if (candidate.source === "mcp_image_content") {
    const extension = MIME_EXTENSIONS.get(candidate.mimeType) ?? "image";
    return {
      bytes: decodeExternalBase64(candidate.data),
      mimeType: candidate.mimeType,
      filename: `mcp-image-${candidate.mediaIndex + 1}.${extension}`,
    };
  }
  const mimeType = mimeTypeFromPath(candidate.path);
  const before = await stat(candidate.path);
  const bytes = await readFile(candidate.path);
  const after = await stat(candidate.path);
  if (
    before.dev !== after.dev ||
    before.ino !== after.ino ||
    before.size !== after.size ||
    before.mtimeMs !== after.mtimeMs
  ) {
    throw new ToolMediaAttachmentError(
      "external_media_changed_before_capture",
      "image-view source changed while Crew was capturing it",
    );
  }
  return {
    bytes,
    mimeType,
    filename: basename(candidate.path),
  };
}

function decodeExternalBase64(value: string): Buffer {
  return value.trim().length === 0 ? Buffer.alloc(0) : decodeBase64(value);
}

function mimeTypeFromPath(path: string): string {
  const extension = extname(path).toLowerCase();
  for (const [mimeType, candidateExtension] of MIME_EXTENSIONS) {
    if (
      extension === `.${candidateExtension}` ||
      (mimeType === "image/jpeg" && extension === ".jpeg")
    ) {
      return mimeType;
    }
  }
  throw new ToolMediaAttachmentError(
    "unsupported_image_mime_type",
    "image-view path does not use a supported image extension",
  );
}

function externalCaptureFailure(
  candidate: ExternalRuntimeMediaCaptureInput["candidates"][number],
  error: unknown,
): ExternalRuntimeMediaReference {
  const reasonCode =
    error instanceof ToolMediaAttachmentError
      ? error.reasonCode
      : isNodeError(error) && error.code === "ENOENT"
        ? "external_media_source_unavailable"
        : "external_media_capture_failed";
  return {
    mediaIndex: candidate.mediaIndex,
    captureSource: candidate.source,
    captureState:
      reasonCode === "unsupported_image_mime_type" ||
      reasonCode === "external_media_data_url_unsupported"
        ? "unsupported"
        : reasonCode === "external_media_source_unavailable" ||
            reasonCode === "external_media_changed_before_capture"
          ? "unavailable"
          : "failed",
    reasonCode,
  };
}

function isNodeError(error: unknown): error is NodeJS.ErrnoException {
  return error instanceof Error && "code" in error;
}

function isToolMediaSource(value: unknown): boolean {
  return (
    value === "brain_tool_media" ||
    value === "external_runtime_media" ||
    value === "chat_upload"
  );
}

function mediaReference(attachment: AttachmentRecord): BrainToolMediaReference {
  const metadata = recordValue(attachment.metadata_json);
  return {
    attachmentId: attachment.attachment_id,
    filename: attachment.filename,
    mimeType: attachment.mime_type,
    byteSize: attachment.byte_size,
    width: Number(metadata.width),
    height: Number(metadata.height),
    downloadUrl: attachment.download_url ?? "",
  };
}
