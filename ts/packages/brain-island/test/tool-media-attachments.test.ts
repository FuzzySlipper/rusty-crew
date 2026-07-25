import assert from "node:assert/strict";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import type { SessionId } from "@rusty-crew/contracts";
import { Type } from "typebox";
import type { BrainTool } from "../src/brain-tool.js";
import type {
  AttachmentRecord,
  ChatEvent,
} from "../src/rusty-view-chat-api.js";
import { rustyViewToolCallDebugDetail } from "../src/service-rusty-view-chat-operations.js";
import { MemoryToolCallDebugStore } from "../src/tool-call-debug-store.js";
import {
  brainToolResultToHostOutput,
  executePreparedBrainHostToolRequest,
  prepareBrainHostToolRequest,
} from "../src/tool-execution-host.js";
import {
  ToolMediaAttachmentError,
  ToolMediaAttachmentStore,
} from "../src/tool-media-attachments.js";
import { mcpToolExecutionResultToBrainResult } from "../src/mcp-brain-tools.js";

function png(width: number, height: number): Buffer {
  const bytes = Buffer.alloc(24);
  bytes.write("\x89PNG\r\n\x1a\n", 0, "binary");
  bytes.write("IHDR", 12, "ascii");
  bytes.writeUInt32BE(width, 16);
  bytes.writeUInt32BE(height, 20);
  return bytes;
}

function harness(rootDir: string, now = "2026-07-25T12:00:00.000Z") {
  const attachments = new Map<string, AttachmentRecord>();
  const events: ChatEvent[] = [];
  let sequence = 0;
  const bridge = {
    async createChatAttachment(input: unknown) {
      const write = (input as { attachment: Record<string, unknown> })
        .attachment;
      const existing = attachments.get(String(write.attachment_id));
      const link = write.link as AttachmentRecord["links"][number] | null;
      const record = {
        ...write,
        links: [
          ...(existing?.links ?? []),
          ...(link &&
          !(existing?.links ?? []).some((item) => item.link_id === link.link_id)
            ? [link]
            : []),
        ],
      } as AttachmentRecord;
      delete (record as unknown as Record<string, unknown>).link;
      attachments.set(record.attachment_id, record);
      return { status: existing ? "updated" : "created", attachment: record };
    },
    async queryAttachmentsPage(input: unknown) {
      const query = input as { session_id?: string };
      const items = [...attachments.values()].filter(
        (record) => !query.session_id || record.session_id === query.session_id,
      );
      return { items, total: items.length, limit: 1_000, offset: 0 };
    },
    async removeChatAttachment(input: unknown) {
      const request = input as { attachment_id: string; updated_at: string };
      const record = attachments.get(request.attachment_id);
      if (!record) throw new Error("not found");
      const removed = {
        ...record,
        status: "removed" as const,
        updated_at: request.updated_at,
      };
      attachments.set(removed.attachment_id, removed);
      return removed;
    },
  };
  const createStore = () =>
    new ToolMediaAttachmentStore({
      artifactDir: rootDir,
      bridge: bridge as never,
      now: () => now,
      appendChatEvent: async (sessionId: SessionId, event) => {
        const saved: ChatEvent = {
          event_id: `${sessionId}:${++sequence}`,
          session_id: sessionId,
          sequence_id: sequence,
          created_at: now,
          kind: event.kind,
          payload: event.payload,
        };
        events.push(saved);
        return saved;
      },
    });
  return { attachments, bridge, createStore, events };
}

test("typed tool images persist, replay after restart, link, and remove without base64 events", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-tool-media-"));
  const testHarness = harness(root);
  const image = png(2, 3);
  const imageData = image.toString("base64");
  const result = {
    content: [
      { type: "text" as const, text: "generated" },
      { type: "image" as const, data: imageData, mimeType: "image/png" },
    ],
    details: { provenance: { adapter: "fake", workflow_id: "portrait-v1" } },
  };
  const [reference] = await testHarness.createStore().persistImages({
    sessionId: "session-1",
    wakeId: "wake-1",
    callId: "call-1",
    toolName: "image_generate",
    result,
  });
  assert.ok(reference);
  assert.equal(reference.width, 2);
  assert.equal(reference.height, 3);
  assert.equal(reference.byteSize, image.length);
  const duplicate = await testHarness.createStore().persistImages({
    sessionId: "session-1",
    wakeId: "wake-1",
    callId: "call-1",
    toolName: "image_generate",
    result,
  });
  assert.deepEqual(duplicate, [reference]);
  assert.equal(
    testHarness.events.filter((event) => event.kind === "attachment_uploaded")
      .length,
    1,
  );
  assert.equal(JSON.stringify(testHarness.events).includes(imageData), false);

  const restarted = testHarness.createStore();
  const content = await restarted.readContent(
    "session-1",
    reference.attachmentId,
  );
  assert.deepEqual(content.bytes, image);
  await restarted.linkAttachmentsToMessage({
    sessionId: "session-1",
    wakeId: "wake-1",
    messageId: "assistant-message-1",
    blockIdsByAttachmentId: new Map([[reference.attachmentId, "block-1"]]),
  });
  const linked = testHarness.attachments.get(reference.attachmentId);
  assert.equal(linked?.links[0]?.message_id, "assistant-message-1");
  assert.equal(linked?.links[0]?.block_id, "block-1");

  const hostOutput = brainToolResultToHostOutput(result, [reference]);
  assert.match(hostOutput, /attachment_id=/);
  assert.equal(hostOutput.includes(imageData), false);
  await restarted.removeContent(linked!);
  await assert.rejects(
    restarted.readContent("session-1", reference.attachmentId),
    /ENOENT/,
  );
});

test("tool image validation rejects malformed, oversized, and unsupported media", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-tool-media-invalid-"));
  const store = new ToolMediaAttachmentStore({
    artifactDir: root,
    bridge: harness(root).bridge as never,
    now: () => "2026-07-25T12:00:00.000Z",
    maxImageBytes: 16,
    appendChatEvent: async () => {
      throw new Error("must not append invalid media");
    },
  });
  const attempt = (data: string, mimeType: string) =>
    store.persistImages({
      sessionId: "session-1",
      wakeId: "wake-1",
      callId: "call-1",
      toolName: "image_generate",
      result: { content: [{ type: "image", data, mimeType }], details: {} },
    });
  await assert.rejects(
    attempt("not-base64", "image/png"),
    ToolMediaAttachmentError,
  );
  await assert.rejects(
    attempt(Buffer.alloc(20).toString("base64"), "image/png"),
    /outside/,
  );
  await assert.rejects(
    attempt(png(1, 1).toString("base64"), "image/svg+xml"),
    /unsupported/,
  );
});

test("expired tool media is inaccessible and its stored bytes are purged", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-tool-media-expiry-"));
  const testHarness = harness(root);
  const [reference] = await testHarness.createStore().persistImages({
    sessionId: "session-1",
    wakeId: "wake-expired",
    callId: "call-expired",
    toolName: "image_generate",
    result: {
      content: [
        {
          type: "image",
          data: png(1, 1).toString("base64"),
          mimeType: "image/png",
        },
      ],
      details: {},
    },
  });
  assert.ok(reference);
  const record = testHarness.attachments.get(reference.attachmentId)!;
  testHarness.attachments.set(reference.attachmentId, {
    ...record,
    expires_at: "2026-07-25T11:59:59.000Z",
  });
  await assert.rejects(
    testHarness.createStore().readContent("session-1", reference.attachmentId),
    (error: unknown) =>
      error instanceof ToolMediaAttachmentError &&
      error.reasonCode === "attachment_expired",
  );
});

test("MCP image content remains typed for the durable media sink", () => {
  const image = png(4, 5).toString("base64");
  const result = mcpToolExecutionResultToBrainResult({
    content: [{ type: "image", data: image, mimeType: "image/png" }],
    details: { source: "mcp" },
  });
  assert.deepEqual(result.content, [
    { type: "image", data: image, mimeType: "image/png" },
  ]);
});

test("production tool debug projection redacts large image bytes and retains attachment references", async () => {
  const root = await mkdtemp(join(tmpdir(), "rusty-tool-media-debug-"));
  const testHarness = harness(root);
  const marker = "RAW_IMAGE_BODY_MUST_NOT_LEAK";
  const imageData = Buffer.concat([
    png(2, 3),
    Buffer.from(marker.repeat(2_000)),
  ]).toString("base64");
  const debugStore = new MemoryToolCallDebugStore({
    maxJsonChars: 1_000,
    now: () => "2026-07-25T12:00:00.000Z",
  });
  const result = {
    content: [
      {
        type: "image" as const,
        data: imageData,
        mimeType: "image/png",
      },
    ],
    details: { status: "completed", padding: "x".repeat(3_000) },
  };
  const tool: BrainTool = {
    name: "image_debug_probe",
    label: "Image debug probe",
    description: "Returns a large typed image for debug projection testing.",
    parameters: Type.Object({}),
    async execute(_callId, _params, _signal, onUpdate) {
      onUpdate?.({
        content: result.content,
        details: { status: "running", padding: "x".repeat(3_000) },
      });
      return result;
    },
  };
  const wake = {
    wakeId: "wake-debug",
    sessionId: "session-debug",
  } as never;
  const prepared = prepareBrainHostToolRequest(
    wake,
    {
      wakeId: "wake-debug",
      callId: "call-debug",
      name: tool.name,
      argumentsJson: "{}",
    },
    new Map([[tool.name, tool]]),
    debugStore,
  );

  const execution = await executePreparedBrainHostToolRequest(
    wake,
    prepared,
    debugStore,
    testHarness.createStore(),
  );
  assert.equal(execution.failure, undefined);
  assert.equal(execution.output.includes(imageData), false);

  const attachment = [...testHarness.attachments.values()][0];
  assert.ok(attachment);
  const detail = await rustyViewToolCallDebugDetail(
    { toolCallDebugStore: debugStore } as never,
    {
      session: { sessionId: "session-debug" } as never,
      debugDetailId: prepared.debugDetailId!,
      requestId: "request-debug",
    },
  );
  assert.ok(detail);
  const serialized = JSON.stringify(detail);
  assert.equal(serialized.includes(imageData.slice(0, 160)), false);
  assert.equal(serialized.includes(marker), false);
  assert.match(serialized, /\[redacted media bytes\]/);
  assert.match(serialized, new RegExp(attachment.attachment_id));
  const debugValues = detail as unknown as {
    partial_updates: Array<{ partial_result: { redacted: boolean } }>;
    final_result?: { redacted: boolean; truncated: boolean };
  };
  assert.equal(debugValues.partial_updates[0]?.partial_result.redacted, true);
  assert.equal(debugValues.final_result?.redacted, true);
  assert.equal(debugValues.final_result?.truncated, true);
});
