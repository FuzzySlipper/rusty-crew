import { createHash } from "node:crypto";
import { mkdir, readFile, rename, rm, stat, writeFile } from "node:fs/promises";
import { basename, extname, join } from "node:path";
import type { SessionId } from "@rusty-crew/contracts";
import type { NativeBridgeModule } from "@rusty-crew/native-bridge";
import sharp from "sharp";
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
import type {
  ExternalRuntimeDocumentCaptureInput,
  ExternalRuntimeDocumentCaptureSink,
  ExternalRuntimeDocumentReference,
} from "./external-runtime-document.js";

export const MAX_CHAT_IMAGE_BYTES = 20 * 1024 * 1024;
export const MAX_EXTERNAL_DOCUMENT_BYTES = 1024 * 1024;
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
  maxDocumentBytes?: number;
}

export interface ToolMediaAttachmentContent {
  attachment: AttachmentRecord;
  bytes: Buffer;
}

export class ToolMediaAttachmentStore
  implements
    BrainToolMediaSink,
    ExternalRuntimeMediaCaptureSink,
    ExternalRuntimeDocumentCaptureSink
{
  private readonly rootDir: string;
  private readonly maxImageBytes: number;
  private readonly maxDocumentBytes: number;

  constructor(private readonly options: ToolMediaAttachmentStoreOptions) {
    this.rootDir = join(options.artifactDir, "tool-media");
    this.maxImageBytes = options.maxImageBytes ?? MAX_CHAT_IMAGE_BYTES;
    this.maxDocumentBytes =
      options.maxDocumentBytes ?? MAX_EXTERNAL_DOCUMENT_BYTES;
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

  async captureExternalRuntimeDocuments(
    input: ExternalRuntimeDocumentCaptureInput,
  ): Promise<readonly ExternalRuntimeDocumentReference[]> {
    if (input.sessionId === undefined) {
      return input.candidates.map((candidate) => ({
        documentIndex: candidate.documentIndex,
        captureSource: candidate.source,
        captureState: "failed",
        reasonCode: "external_document_session_unbound",
      }));
    }
    const references: ExternalRuntimeDocumentReference[] = [];
    for (const candidate of input.candidates) {
      try {
        const identity = [
          input.runtimeId,
          input.bindingId ?? input.sessionId,
          input.nativeThreadId ?? "thread-unknown",
          input.nativeTurnId ?? "turn-unknown",
          input.itemId ?? input.externalEventId,
          String(candidate.documentIndex),
        ].join(":");
        const existing = await this.findAttachmentOptional(
          input.sessionId,
          stableId("attachment", identity),
        );
        if (existing !== undefined && existing.status === "active") {
          const metadata = recordValue(existing.metadata_json);
          await readFile(this.pathFromAttachment(existing));
          references.push({
            documentIndex: candidate.documentIndex,
            captureSource: candidate.source,
            captureState: "available",
            attachmentId: existing.attachment_id,
            filename: existing.filename,
            mimeType: existing.mime_type,
            ...(typeof metadata.language_hint === "string"
              ? { languageHint: metadata.language_hint }
              : {}),
            byteSize: existing.byte_size,
            ...(typeof metadata.content_sha256 === "string"
              ? { sha256: metadata.content_sha256 }
              : {}),
            contentUrl: existing.download_url ?? "",
          });
          continue;
        }
        const descriptor = documentDescriptor(candidate.path);
        if (descriptor === undefined) {
          references.push(documentCaptureFailure(candidate, "unsupported"));
          continue;
        }
        const before = await stat(candidate.path).catch((error) => {
          throw documentReadError(error);
        });
        if (!before.isFile()) {
          references.push(documentCaptureFailure(candidate, "unsupported"));
          continue;
        }
        if (before.size > this.maxDocumentBytes) {
          references.push({
            documentIndex: candidate.documentIndex,
            captureSource: candidate.source,
            captureState: "oversized",
            reasonCode: "external_document_oversized",
            filename: basename(candidate.path),
            mimeType: descriptor.mimeType,
            languageHint: descriptor.languageHint,
            byteSize: before.size,
          });
          continue;
        }
        const bytes = await readFile(candidate.path).catch((error) => {
          throw documentReadError(error);
        });
        const after = await stat(candidate.path).catch((error) => {
          throw documentReadError(error);
        });
        if (
          before.dev !== after.dev ||
          before.ino !== after.ino ||
          before.size !== after.size ||
          before.mtimeMs !== after.mtimeMs ||
          bytes.length !== before.size
        ) {
          references.push(documentCaptureFailure(candidate, "changed"));
          continue;
        }
        if (bytes.length === 0) {
          references.push(documentCaptureFailure(candidate, "empty"));
          continue;
        }
        if (!isUtf8Text(bytes)) {
          references.push(documentCaptureFailure(candidate, "binary"));
          continue;
        }
        const sha256 = createHash("sha256").update(bytes).digest("hex");
        const stored = await this.persistStoredDocument({
          sessionId: input.sessionId,
          identity,
          filename: basename(candidate.path),
          extension: descriptor.extension,
          mimeType: descriptor.mimeType,
          bytes,
          metadata: {
            source: "external_runtime_document",
            runtime_id: input.runtimeId,
            binding_id: input.bindingId ?? null,
            native_thread_id: input.nativeThreadId ?? null,
            native_turn_id: input.nativeTurnId ?? null,
            item_id: input.itemId ?? null,
            external_event_id: input.externalEventId,
            document_index: candidate.documentIndex,
            capture_source: candidate.source,
            language_hint: descriptor.languageHint,
          },
        });
        references.push({
          documentIndex: candidate.documentIndex,
          captureSource: candidate.source,
          captureState: "available",
          attachmentId: stored.attachmentId,
          filename: stored.filename,
          mimeType: descriptor.mimeType,
          languageHint: descriptor.languageHint,
          byteSize: bytes.length,
          sha256,
          contentUrl: stored.downloadUrl,
        });
      } catch (error) {
        references.push(
          documentCaptureFailure(
            candidate,
            error instanceof ToolMediaAttachmentError &&
              error.reasonCode === "external_document_missing"
              ? "missing"
              : "failed",
            error instanceof ToolMediaAttachmentError
              ? error.reasonCode
              : "external_document_capture_failed",
          ),
        );
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
    if (input.bytes.length === 0 || input.bytes.length > this.maxImageBytes) {
      throw new ToolMediaAttachmentError(
        "invalid_image_byte_size",
        `chat image byte size ${input.bytes.length} is outside 1..${this.maxImageBytes}`,
      );
    }
    imageDimensions(input.bytes, input.mimeType);
    await validateDecodableUpload(input.bytes, input.mimeType);
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

  async resolveExternalInputImage(
    sessionId: string,
    storageUrl: string,
  ): Promise<string> {
    const attachments = await this.queryAllAttachments({
      session_id: sessionId,
      include_removed: true,
      include_expired: true,
      expired_only: false,
      now: this.options.now(),
    });
    const attachment = attachments.find(
      (candidate) => candidate.storage_url === storageUrl,
    );
    if (attachment === undefined) {
      throw new ToolMediaAttachmentError(
        "external_message_image_not_found",
        "external input image no longer matches the bound session",
      );
    }
    if (attachment.status !== "active") {
      throw new ToolMediaAttachmentError(
        "external_message_image_inactive",
        `attachment ${attachment.attachment_id} is removed`,
      );
    }
    const path = this.pathFromAttachment(attachment);
    await readFile(path);
    return path;
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

  private async persistStoredDocument(input: {
    sessionId: string;
    identity: string;
    filename: string;
    extension: string;
    mimeType: string;
    bytes: Buffer;
    metadata: Record<string, unknown>;
  }): Promise<{
    attachmentId: string;
    filename: string;
    downloadUrl: string;
  }> {
    const attachmentId = stableId("attachment", input.identity);
    const filename = safeDisplayFilename(input.filename, input.extension);
    const sessionDir = join(this.rootDir, digest(input.sessionId));
    const finalPath = join(
      sessionDir,
      `${digest(attachmentId)}.${input.extension}`,
    );
    const temporaryPath = `${finalPath}.tmp-${process.pid}-${Date.now()}`;
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
          `document attachment ${attachmentId} conflicts with an existing record`,
        );
      }
      await readFile(this.pathFromAttachment(existing));
      return { attachmentId, filename: existing.filename, downloadUrl };
    }
    await mkdir(sessionDir, { recursive: true });
    await writeFile(temporaryPath, input.bytes, { flag: "wx", mode: 0o600 });
    await rename(temporaryPath, finalPath).catch(async (error) => {
      await rm(temporaryPath, { force: true }).catch(() => undefined);
      throw error;
    });
    try {
      const now = this.options.now();
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
            storage_extension: input.extension,
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
      return { attachmentId, filename, downloadUrl };
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
    const metadata = recordValue(attachment.metadata_json);
    const storedExtension = metadata.storage_extension;
    const extension =
      typeof storedExtension === "string" && /^[a-z0-9]+$/.test(storedExtension)
        ? storedExtension
        : MIME_EXTENSIONS.get(attachment.mime_type);
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

async function validateDecodableUpload(
  bytes: Buffer,
  mimeType: string,
): Promise<void> {
  const expectedFormat =
    mimeType === "image/png"
      ? "png"
      : mimeType === "image/jpeg"
        ? "jpeg"
        : mimeType === "image/webp"
          ? "webp"
          : undefined;
  if (expectedFormat === undefined) return;
  try {
    const decoded = await sharp(bytes, {
      failOn: "warning",
      limitInputPixels: 25_000_000,
      sequentialRead: true,
    })
      .raw()
      .toBuffer({ resolveWithObject: true });
    if (
      decoded.info.format !== "raw" ||
      decoded.info.width < 1 ||
      decoded.info.height < 1
    ) {
      throw new Error("decoder returned invalid image metadata");
    }
    const metadata = await sharp(bytes, {
      failOn: "warning",
      limitInputPixels: 25_000_000,
      sequentialRead: true,
    }).metadata();
    if (metadata.format !== expectedFormat) {
      throw new Error(
        `declared ${mimeType} content decoded as ${metadata.format ?? "unknown"}`,
      );
    }
  } catch (error) {
    throw new ToolMediaAttachmentError(
      "invalid_image_content",
      `image bytes are not a decodable ${mimeType}: ${error instanceof Error ? error.message : String(error)}`,
    );
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
  if (mimeType === "image/png") {
    ({ width, height } = pngDimensions(bytes));
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

function pngDimensions(bytes: Buffer): { width: number; height: number } {
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  if (bytes.length < 45 || !bytes.subarray(0, 8).equals(signature)) {
    return { width: 0, height: 0 };
  }
  let offset = 8;
  let width = 0;
  let height = 0;
  let sawHeader = false;
  let sawImageData = false;
  while (offset + 12 <= bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const dataStart = offset + 8;
    const crcOffset = dataStart + length;
    const chunkEnd = crcOffset + 4;
    if (chunkEnd > bytes.length) return { width: 0, height: 0 };
    const type = bytes.subarray(offset + 4, dataStart).toString("ascii");
    if (
      bytes.readUInt32BE(crcOffset) !==
      pngCrc32(bytes.subarray(offset + 4, crcOffset))
    ) {
      return { width: 0, height: 0 };
    }
    if (!sawHeader) {
      if (type !== "IHDR" || length !== 13 || offset !== 8) {
        return { width: 0, height: 0 };
      }
      width = bytes.readUInt32BE(dataStart);
      height = bytes.readUInt32BE(dataStart + 4);
      const bitDepth = bytes[dataStart + 8] ?? 0;
      const colorType = bytes[dataStart + 9] ?? 0;
      const validDepths = new Map<number, readonly number[]>([
        [0, [1, 2, 4, 8, 16]],
        [2, [8, 16]],
        [3, [1, 2, 4, 8]],
        [4, [8, 16]],
        [6, [8, 16]],
      ]);
      if (
        !validDepths.get(colorType)?.includes(bitDepth) ||
        bytes[dataStart + 10] !== 0 ||
        bytes[dataStart + 11] !== 0 ||
        ![0, 1].includes(bytes[dataStart + 12] ?? -1)
      ) {
        return { width: 0, height: 0 };
      }
      sawHeader = true;
    } else if (type === "IHDR") {
      return { width: 0, height: 0 };
    }
    if (type === "IDAT") {
      if (length === 0) return { width: 0, height: 0 };
      sawImageData = true;
    }
    if (type === "IEND") {
      if (
        length !== 0 ||
        !sawHeader ||
        !sawImageData ||
        chunkEnd !== bytes.length
      ) {
        return { width: 0, height: 0 };
      }
      return { width, height };
    }
    offset = chunkEnd;
  }
  return { width: 0, height: 0 };
}

function pngCrc32(bytes: Buffer): number {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc = (crc >>> 8) ^ (PNG_CRC_TABLE[(crc ^ byte) & 0xff] ?? 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}

const PNG_CRC_TABLE = Array.from({ length: 256 }, (_, value) => {
  let crc = value;
  for (let bit = 0; bit < 8; bit += 1) {
    crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return crc >>> 0;
});

function jpegDimensions(bytes: Buffer): { width: number; height: number } {
  if (
    bytes.length < 12 ||
    bytes[0] !== 0xff ||
    bytes[1] !== 0xd8 ||
    bytes[bytes.length - 2] !== 0xff ||
    bytes[bytes.length - 1] !== 0xd9
  )
    return { width: 0, height: 0 };
  let offset = 2;
  let width = 0;
  let height = 0;
  while (offset + 4 <= bytes.length - 2) {
    if (bytes[offset] !== 0xff) {
      return { width: 0, height: 0 };
    }
    while (bytes[offset] === 0xff) offset += 1;
    const marker = bytes[offset] ?? 0;
    offset += 1;
    if (marker === 0xda) {
      if (offset + 2 > bytes.length - 2) return { width: 0, height: 0 };
      const length = bytes.readUInt16BE(offset);
      const scanStart = offset + length;
      return width > 0 &&
        height > 0 &&
        length >= 6 &&
        scanStart < bytes.length - 2
        ? { width, height }
        : { width: 0, height: 0 };
    }
    if (marker === 0x00 || marker === 0xd8 || marker === 0xd9) {
      return { width: 0, height: 0 };
    }
    if (offset + 2 > bytes.length - 2) return { width: 0, height: 0 };
    const length = bytes.readUInt16BE(offset);
    if (length < 2 || offset + length > bytes.length - 2) {
      return { width: 0, height: 0 };
    }
    if (
      [
        0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce,
        0xcf,
      ].includes(marker)
    ) {
      if (length < 8) return { width: 0, height: 0 };
      height = bytes.readUInt16BE(offset + 3);
      width = bytes.readUInt16BE(offset + 5);
    }
    offset += length;
  }
  return { width: 0, height: 0 };
}

function webpDimensions(bytes: Buffer): { width: number; height: number } {
  if (
    bytes.length < 20 ||
    bytes.subarray(0, 4).toString() !== "RIFF" ||
    bytes.subarray(8, 12).toString() !== "WEBP" ||
    bytes.readUInt32LE(4) + 8 !== bytes.length
  )
    return { width: 0, height: 0 };
  let offset = 12;
  let canvas: { width: number; height: number } | undefined;
  let image: { width: number; height: number } | undefined;
  while (offset + 8 <= bytes.length) {
    const kind = bytes.subarray(offset, offset + 4).toString("ascii");
    const length = bytes.readUInt32LE(offset + 4);
    const dataStart = offset + 8;
    const dataEnd = dataStart + length;
    const chunkEnd = dataEnd + (length & 1);
    if (dataEnd > bytes.length || chunkEnd > bytes.length) {
      return { width: 0, height: 0 };
    }
    if (kind === "VP8X") {
      if (length !== 10 || canvas !== undefined) return { width: 0, height: 0 };
      canvas = {
        width: 1 + bytes.readUIntLE(dataStart + 4, 3),
        height: 1 + bytes.readUIntLE(dataStart + 7, 3),
      };
    } else if (kind === "VP8 ") {
      if (
        length < 10 ||
        !bytes
          .subarray(dataStart + 3, dataStart + 6)
          .equals(Buffer.from([0x9d, 0x01, 0x2a]))
      ) {
        return { width: 0, height: 0 };
      }
      image = {
        width: bytes.readUInt16LE(dataStart + 6) & 0x3fff,
        height: bytes.readUInt16LE(dataStart + 8) & 0x3fff,
      };
    } else if (kind === "VP8L") {
      if (length < 5 || bytes[dataStart] !== 0x2f) {
        return { width: 0, height: 0 };
      }
      const b0 = bytes[dataStart + 1] ?? 0;
      const b1 = bytes[dataStart + 2] ?? 0;
      const b2 = bytes[dataStart + 3] ?? 0;
      const b3 = bytes[dataStart + 4] ?? 0;
      image = {
        width: 1 + b0 + ((b1 & 0x3f) << 8),
        height: 1 + (b1 >> 6) + (b2 << 2) + ((b3 & 0x0f) << 10),
      };
    }
    offset = chunkEnd;
  }
  if (offset !== bytes.length || image === undefined) {
    return { width: 0, height: 0 };
  }
  if (
    canvas !== undefined &&
    (canvas.width !== image.width || canvas.height !== image.height)
  ) {
    return { width: 0, height: 0 };
  }
  return canvas ?? image;
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

const DOCUMENT_DESCRIPTORS = new Map<
  string,
  { mimeType: string; languageHint: string }
>([
  ["md", { mimeType: "text/markdown", languageHint: "markdown" }],
  ["markdown", { mimeType: "text/markdown", languageHint: "markdown" }],
  ["mdown", { mimeType: "text/markdown", languageHint: "markdown" }],
  ["txt", { mimeType: "text/plain", languageHint: "text" }],
  ["rs", { mimeType: "text/x-rust", languageHint: "rust" }],
  ["ts", { mimeType: "text/typescript", languageHint: "typescript" }],
  ["tsx", { mimeType: "text/typescript", languageHint: "tsx" }],
  ["js", { mimeType: "text/javascript", languageHint: "javascript" }],
  ["jsx", { mimeType: "text/javascript", languageHint: "jsx" }],
  ["json", { mimeType: "application/json", languageHint: "json" }],
  ["toml", { mimeType: "application/toml", languageHint: "toml" }],
  ["yaml", { mimeType: "application/yaml", languageHint: "yaml" }],
  ["yml", { mimeType: "application/yaml", languageHint: "yaml" }],
  ["css", { mimeType: "text/css", languageHint: "css" }],
  ["scss", { mimeType: "text/x-scss", languageHint: "scss" }],
  ["html", { mimeType: "text/html", languageHint: "html" }],
  ["xml", { mimeType: "application/xml", languageHint: "xml" }],
  ["sh", { mimeType: "text/x-shellscript", languageHint: "shell" }],
  ["bash", { mimeType: "text/x-shellscript", languageHint: "shell" }],
  ["py", { mimeType: "text/x-python", languageHint: "python" }],
  ["go", { mimeType: "text/x-go", languageHint: "go" }],
  ["java", { mimeType: "text/x-java-source", languageHint: "java" }],
  ["kt", { mimeType: "text/x-kotlin", languageHint: "kotlin" }],
  ["c", { mimeType: "text/x-c", languageHint: "c" }],
  ["h", { mimeType: "text/x-c", languageHint: "c" }],
  ["cc", { mimeType: "text/x-c++", languageHint: "cpp" }],
  ["cpp", { mimeType: "text/x-c++", languageHint: "cpp" }],
  ["hpp", { mimeType: "text/x-c++", languageHint: "cpp" }],
  ["cs", { mimeType: "text/x-csharp", languageHint: "csharp" }],
  ["rb", { mimeType: "text/x-ruby", languageHint: "ruby" }],
  ["php", { mimeType: "text/x-php", languageHint: "php" }],
  ["sql", { mimeType: "application/sql", languageHint: "sql" }],
  ["graphql", { mimeType: "application/graphql", languageHint: "graphql" }],
  ["proto", { mimeType: "text/x-protobuf", languageHint: "protobuf" }],
  ["swift", { mimeType: "text/x-swift", languageHint: "swift" }],
  ["vue", { mimeType: "text/x-vue", languageHint: "vue" }],
  ["svelte", { mimeType: "text/x-svelte", languageHint: "svelte" }],
]);

function documentDescriptor(
  path: string,
): { extension: string; mimeType: string; languageHint: string } | undefined {
  const extension = extname(path).slice(1).toLowerCase();
  const descriptor = DOCUMENT_DESCRIPTORS.get(extension);
  return descriptor === undefined ? undefined : { extension, ...descriptor };
}

function isUtf8Text(bytes: Buffer): boolean {
  if (bytes.includes(0)) return false;
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(bytes);
    return true;
  } catch {
    return false;
  }
}

function documentReadError(error: unknown): ToolMediaAttachmentError {
  return isNodeError(error) &&
    (error.code === "ENOENT" || error.code === "ENOTDIR")
    ? new ToolMediaAttachmentError(
        "external_document_missing",
        "external document no longer exists",
      )
    : new ToolMediaAttachmentError(
        "external_document_read_failed",
        error instanceof Error ? error.message : String(error),
      );
}

function documentCaptureFailure(
  candidate: ExternalRuntimeDocumentCaptureInput["candidates"][number],
  state:
    | "missing"
    | "binary"
    | "empty"
    | "oversized"
    | "changed"
    | "unsupported"
    | "failed",
  reasonCode = `external_document_${state}`,
): ExternalRuntimeDocumentReference {
  return {
    documentIndex: candidate.documentIndex,
    captureSource: candidate.source,
    captureState: state,
    reasonCode,
  };
}

function isToolMediaSource(value: unknown): boolean {
  return (
    value === "brain_tool_media" ||
    value === "external_runtime_media" ||
    value === "external_runtime_document" ||
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
