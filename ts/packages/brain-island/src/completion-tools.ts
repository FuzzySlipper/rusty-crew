import type {
  BrainAction,
  CompletionStatus,
  SessionId,
} from "@rusty-crew/contracts";
import { Type, type Static } from "typebox";
import type { BrainTool, BrainToolResult } from "./brain-tool.js";
import type {
  BrainActionCollector,
  BrainToolResolver,
} from "./tool-session-selection.js";

const completionMarkdownParameters = Type.Object({
  markdown: Type.String({ minLength: 1 }),
});

type CompletionMarkdownParams = Static<typeof completionMarkdownParameters>;

export interface CompletionToolContext {
  actions?: BrainActionCollector;
}

export interface CompletionToolDetails {
  ok: boolean;
  operation: "deliver_completion_md";
  reasonCode?: string;
  queuedActions: number;
  actions: BrainAction[];
  status?: CompletionStatus;
  summary?: string;
}

export const resolveCompletionTools: BrainToolResolver = ({ actions }) =>
  completionTools({ actions });

export function completionTools(context: CompletionToolContext): BrainTool[] {
  return [deliverCompletionMarkdownTool(context)];
}

export function deliverCompletionMarkdownTool(
  context: CompletionToolContext,
): BrainTool<typeof completionMarkdownParameters, CompletionToolDetails> {
  return {
    name: "deliver_completion_md",
    label: "Deliver completion from markdown",
    description:
      "Queue a Rusty Crew completion packet from markdown and simple frontmatter. Put status in the header and a Summary section or concise body in markdown. Do not write JSON.",
    parameters: completionMarkdownParameters,
    executeWithContext: async (params, toolContext) =>
      deliverCompletion(context, toolContext.sessionId, params),
    execute: async (_toolCallId, params) =>
      deliverCompletion(context, undefined, params),
  };
}

function deliverCompletion(
  context: CompletionToolContext,
  sessionId: string | undefined,
  params: CompletionMarkdownParams,
): BrainToolResult<CompletionToolDetails> {
  if (!context.actions) {
    return rejected("completion_action_collector_unavailable");
  }
  if (!sessionId?.trim()) {
    return rejected("completion_session_unavailable");
  }
  const parsed = parseCompletionMarkdown(params.markdown);
  if (!parsed.ok) return rejected(parsed.reasonCode);

  const action: BrainAction = {
    type: "deliver_completion",
    packet: {
      sessionId: sessionId as SessionId,
      status: parsed.status,
      summary: parsed.summary,
    },
  };
  context.actions.add(action);
  return result({
    ok: true,
    operation: "deliver_completion_md",
    queuedActions: 1,
    actions: [action],
    status: parsed.status,
    summary: parsed.summary,
  });
}

type CompletionParseResult =
  | { ok: true; status: CompletionStatus; summary: string }
  | { ok: false; reasonCode: string };

interface MarkdownEnvelope {
  frontmatter: Record<string, string>;
  bodyMarkdown: string;
}

function parseCompletionMarkdown(markdown: string): CompletionParseResult {
  const envelope = parseMarkdownEnvelope(markdown);
  if (!envelope.ok) return envelope;
  const status = parseCompletionStatus(
    stringField(envelope.value.frontmatter, "status"),
  );
  if (!status.ok) return status;
  const summary =
    stringField(envelope.value.frontmatter, "summary") ??
    sectionByHeading(envelope.value.bodyMarkdown, "Summary") ??
    envelope.value.bodyMarkdown.trim();
  if (!summary.trim()) {
    return { ok: false, reasonCode: "completion_summary_required" };
  }
  return {
    ok: true,
    status: status.status,
    summary: summary.trim(),
  };
}

function parseCompletionStatus(
  raw: string | undefined,
): { ok: true; status: CompletionStatus } | { ok: false; reasonCode: string } {
  if (raw === undefined) {
    return { ok: false, reasonCode: "completion_status_required" };
  }
  if (
    raw === "completed" ||
    raw === "failed" ||
    raw === "blocked" ||
    raw === "exhausted"
  ) {
    return { ok: true, status: raw };
  }
  return { ok: false, reasonCode: "invalid_completion_status" };
}

type ParseResult<T> =
  | { ok: true; value: T }
  | { ok: false; reasonCode: string };

function parseMarkdownEnvelope(
  markdown: string,
): ParseResult<MarkdownEnvelope> {
  const normalized = markdown.replace(/\r\n/g, "\n").trim();
  if (!normalized) {
    return { ok: false, reasonCode: "completion_markdown_required" };
  }
  if (!normalized.startsWith("---\n")) {
    return { ok: true, value: { frontmatter: {}, bodyMarkdown: normalized } };
  }
  const closing = normalized.indexOf("\n---", 4);
  if (closing === -1) {
    return { ok: false, reasonCode: "invalid_completion_frontmatter" };
  }
  const closeEnd = closing + "\n---".length;
  const afterClose = normalized.slice(closeEnd);
  if (afterClose.length > 0 && !afterClose.startsWith("\n")) {
    return { ok: false, reasonCode: "invalid_completion_frontmatter" };
  }
  const frontmatter = parseSimpleFrontmatter(normalized.slice(4, closing));
  if (!frontmatter.ok) return frontmatter;
  return {
    ok: true,
    value: {
      frontmatter: frontmatter.value,
      bodyMarkdown: afterClose.trim(),
    },
  };
}

function parseSimpleFrontmatter(
  raw: string,
): ParseResult<Record<string, string>> {
  const fields: Record<string, string> = {};
  for (const line of raw.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const match = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(trimmed);
    if (!match) {
      return { ok: false, reasonCode: "invalid_completion_frontmatter" };
    }
    const value = unquote(match[2]!.trim());
    if (!value) {
      return { ok: false, reasonCode: "invalid_completion_frontmatter" };
    }
    fields[normalizeFieldName(match[1]!)] = value;
  }
  return { ok: true, value: fields };
}

function sectionByHeading(
  markdown: string,
  heading: string,
): string | undefined {
  const pattern = new RegExp(
    `^##\\s+${escapeRegExp(heading)}\\s*$\\n([\\s\\S]*?)(?=^##\\s+|$)`,
    "im",
  );
  return pattern.exec(markdown)?.[1]?.trim() || undefined;
}

function stringField(
  fields: Record<string, string>,
  key: string,
): string | undefined {
  const value = fields[normalizeFieldName(key)]?.trim();
  return value ? value : undefined;
}

function normalizeFieldName(value: string): string {
  return value.trim().toLowerCase().replace(/-/g, "_");
}

function unquote(value: string): string {
  if (
    (value.startsWith('"') && value.endsWith('"')) ||
    (value.startsWith("'") && value.endsWith("'"))
  ) {
    return value.slice(1, -1).trim();
  }
  return value.trim();
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function rejected(reasonCode: string): BrainToolResult<CompletionToolDetails> {
  return result({
    ok: false,
    operation: "deliver_completion_md",
    reasonCode,
    queuedActions: 0,
    actions: [],
  });
}

function result(
  details: CompletionToolDetails,
): BrainToolResult<CompletionToolDetails> {
  return {
    content: [{ type: "text", text: JSON.stringify(details, null, 2) }],
    details,
    terminate: details.ok,
  };
}
