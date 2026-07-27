import { createHash } from "node:crypto";
import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
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

const MAX_TOOL_IMAGE_BYTES = 20 * 1024 * 1024;
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

export class ToolMediaAttachmentStore implements BrainToolMediaSink {
  private readonly rootDir: string;
  private readonly maxImageBytes: number;

  constructor(private readonly options: ToolMediaAttachmentStoreOptions) {
    this.rootDir = join(options.artifactDir, "tool-media");
    this.maxImageBytes = options.maxImageBytes ?? MAX_TOOL_IMAGE_BYTES;
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
    for (const attachment of attachments) {
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
        metadata_json: {
          source: "brain_tool_media",
          wake_id: input.wakeId,
        },
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
    if (recordValue(attachment.metadata_json).source !== "brain_tool_media") {
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
    const extension = MIME_EXTENSIONS.get(input.image.mimeType);
    if (!extension) {
      throw new ToolMediaAttachmentError(
        "unsupported_image_mime_type",
        `unsupported tool image MIME type ${input.image.mimeType}`,
      );
    }
    const bytes = decodeBase64(input.image.data);
    if (bytes.length === 0 || bytes.length > this.maxImageBytes) {
      throw new ToolMediaAttachmentError(
        "invalid_image_byte_size",
        `tool image byte size ${bytes.length} is outside 1..${this.maxImageBytes}`,
      );
    }
    const dimensions = imageDimensions(bytes, input.image.mimeType);
    const attachmentId = stableId(
      "attachment",
      `${input.sessionId}:${input.wakeId}:${input.callId}:${input.imageIndex}`,
    );
    const filename = `${safeStem(input.toolName)}-${input.imageIndex + 1}.${extension}`;
    const sessionDir = join(this.rootDir, digest(input.sessionId));
    const finalPath = join(sessionDir, `${digest(attachmentId)}.${extension}`);
    const temporaryPath = `${finalPath}.tmp-${process.pid}-${Date.now()}`;
    const now = this.options.now();
    const downloadUrl = toolMediaDownloadUrl(input.sessionId, attachmentId);
    const contentSha256 = createHash("sha256").update(bytes).digest("hex");
    const existing = await this.findAttachmentOptional(
      input.sessionId,
      attachmentId,
    );
    if (existing !== undefined) {
      const metadata = recordValue(existing.metadata_json);
      if (
        existing.status !== "active" ||
        existing.mime_type !== input.image.mimeType ||
        existing.byte_size !== bytes.length ||
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
    await writeFile(temporaryPath, bytes, { flag: "wx", mode: 0o600 });
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
          mime_type: input.image.mimeType,
          byte_size: bytes.length,
          storage_url: `artifact://tool-media/${digest(input.sessionId)}/${basename(finalPath)}`,
          download_url: downloadUrl,
          thumbnail_url: null,
          extracted_text: null,
          extracted_text_truncated: false,
          metadata_json: {
            source: "brain_tool_media",
            wake_id: input.wakeId,
            tool_call_id: input.callId,
            tool_name: input.toolName,
            image_index: input.imageIndex,
            width: dimensions.width,
            height: dimensions.height,
            content_sha256: contentSha256,
            provenance: safeProvenance(input.result.details),
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
        mimeType: input.image.mimeType,
        byteSize: bytes.length,
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
    if (metadata.source !== "brain_tool_media") {
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
