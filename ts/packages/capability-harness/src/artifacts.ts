import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import type {
  CapabilityDebugSnapshot,
  CapabilityEvidencePacket,
} from "./model.js";

const REDACTED = "[REDACTED]";
const MAX_STRING_CHARS = 64 * 1024;
const MAX_ARRAY_ITEMS = 1_000;
const SENSITIVE_KEY =
  /^(?:authorization|proxy-authorization|cookie|set-cookie|api[_-]?key|access[_-]?token|refresh[_-]?token|id[_-]?token|client[_-]?secret|password|secret)$/i;

export interface CapabilityArtifactPaths {
  debugSnapshot: string;
  evidencePacket: string;
  scenarioSummary: string;
}

export function redactCapabilityEvidence(value: unknown): unknown {
  return redactValue(value, new WeakSet<object>());
}

export async function writeCapabilityArtifacts(
  directory: string,
  packet: CapabilityEvidencePacket,
  rawByRuntime: Record<string, unknown>,
): Promise<CapabilityArtifactPaths> {
  await mkdir(directory, { recursive: true });
  const evidencePacket = join(directory, "evidence-packet.json");
  const debugSnapshot = join(directory, "debug-snapshot.json");
  const scenarioSummary = join(directory, "scenario-summary.md");
  const snapshot: CapabilityDebugSnapshot = {
    schemaVersion: packet.schemaVersion,
    runId: packet.runId,
    capturedAt: new Date().toISOString(),
    rawByRuntime: redactCapabilityEvidence(rawByRuntime) as Record<
      string,
      unknown
    >,
  };
  await Promise.all([
    writeFile(
      evidencePacket,
      `${JSON.stringify(redactCapabilityEvidence(packet), null, 2)}\n`,
      "utf8",
    ),
    writeFile(debugSnapshot, `${JSON.stringify(snapshot, null, 2)}\n`, "utf8"),
    writeFile(scenarioSummary, renderScenarioSummary(packet), "utf8"),
  ]);
  return { debugSnapshot, evidencePacket, scenarioSummary };
}

export function renderScenarioSummary(
  packet: CapabilityEvidencePacket,
): string {
  const lines = [
    `# ${packet.scenario.title}`,
    "",
    `Run: \`${packet.runId}\``,
    `Scenario: \`${packet.scenario.id}\``,
    "",
    "## Runtime Results",
    "",
    "| Runtime | Kind | Passed | Duration | Interactions | Recovery |",
    "| --- | --- | --- | ---: | ---: | --- |",
  ];
  for (const runtime of packet.runtimes) {
    lines.push(
      `| ${runtime.runtimeId} | ${runtime.runtimeKind} | ${packet.comparison.scenarioPassedByRuntime[runtime.runtimeId] === true ? "yes" : "no"} | ${runtime.durationMs} ms | ${runtime.interactions.length} | ${packet.comparison.recovery[runtime.runtimeId]} |`,
    );
  }
  lines.push("", "## Unsupported Capabilities", "");
  for (const runtime of packet.runtimes) {
    const unsupported =
      packet.comparison.unsupportedCapabilities[runtime.runtimeId] ?? [];
    lines.push(
      `- **${runtime.runtimeId}:** ${unsupported.length === 0 ? "none" : unsupported.join(", ")}`,
    );
  }
  lines.push("", "## Validation", "");
  for (const command of packet.scenario.validationCommands) {
    lines.push(`- \`${command}\``);
  }
  lines.push("");
  return lines.join("\n");
}

function redactValue(value: unknown, seen: WeakSet<object>): unknown {
  if (typeof value === "string") return redactString(value);
  if (value === null || typeof value !== "object") return value;
  if (seen.has(value)) return "[CIRCULAR]";
  seen.add(value);
  if (Array.isArray(value)) {
    const output = value
      .slice(0, MAX_ARRAY_ITEMS)
      .map((item) => redactValue(item, seen));
    seen.delete(value);
    return output;
  }
  const output: Record<string, unknown> = {};
  for (const [key, item] of Object.entries(value)) {
    output[key] = SENSITIVE_KEY.test(key) ? REDACTED : redactValue(item, seen);
  }
  seen.delete(value);
  return output;
}

function redactString(value: string): string {
  const bounded =
    value.length > MAX_STRING_CHARS
      ? `${value.slice(0, MAX_STRING_CHARS)}[TRUNCATED]`
      : value;
  return bounded
    .replace(/\bBearer\s+[A-Za-z0-9._~+\/-]+=*/gi, `Bearer ${REDACTED}`)
    .replace(/\b(?:sk|sess|ac)_[A-Za-z0-9._-]{12,}\b/g, REDACTED);
}
