import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { hrtime } from "node:process";

import type { SessionId } from "@rusty-crew/contracts";

import { loadNativeBridge } from "./index.js";

interface RunResult {
  producerBatchSize: number;
  events: number;
  textBytes: number;
  totalMs: number;
  eventsPerSecond: number;
  latencyMs: {
    p50: number;
    p95: number;
    p99: number;
    max: number;
  };
  rssDeltaBytes: number;
  cpuMicros: {
    user: number;
    system: number;
    total: number;
  };
}

const events = parsePositiveInteger("RUSTY_CREW_THROUGHPUT_EVENTS", 10_000);
const textBytes = parsePositiveInteger("RUSTY_CREW_THROUGHPUT_TEXT_BYTES", 64);
const batchSizes = parseBatchSizes(
  process.env.RUSTY_CREW_THROUGHPUT_BATCHES ?? "1,16,64",
);

const results: RunResult[] = [];

for (const producerBatchSize of batchSizes) {
  results.push(await measureRun(producerBatchSize));
}

console.log(
  JSON.stringify(
    {
      label: "true napi bridge throughput",
      date: new Date().toISOString(),
      eventsPerRun: events,
      textBytes,
      results,
    },
    null,
    2,
  ),
);

async function measureRun(producerBatchSize: number): Promise<RunResult> {
  const bridge = await loadNativeBridge();
  const engineDataDir = mkdtempSync(
    join(tmpdir(), "rusty-crew-napi-throughput-"),
  );
  const sessionId = `throughput-session-${producerBatchSize}` as SessionId;
  const wakeId = `throughput-wake-${producerBatchSize}`;
  const text = "x".repeat(textBytes);

  const engine = await bridge.initializeEngine({
    engineDataDir,
    clock: { fixed: "2026-06-19T00:00:00Z" },
    defaultTurnBudget: 3,
    defaultIdleTimeoutMs: 1_000,
  });

  try {
    await bridge.submitBrainEvent({
      wakeId,
      sessionId,
      event: { type: "started" },
    });

    const latenciesNs = new Array<number>(events);
    const rssStart = process.memoryUsage().rss;
    const cpuStart = process.cpuUsage();
    const totalStart = hrtime.bigint();

    for (let offset = 0; offset < events; offset += producerBatchSize) {
      const batchEnd = Math.min(offset + producerBatchSize, events);
      for (let index = offset; index < batchEnd; index += 1) {
        const submitStart = hrtime.bigint();
        await bridge.submitBrainEvent({
          wakeId,
          sessionId,
          event: { type: "text_delta", text },
        });
        latenciesNs[index] = Number(hrtime.bigint() - submitStart);
      }
      await Promise.resolve();
    }

    const totalNs = Number(hrtime.bigint() - totalStart);
    const cpu = process.cpuUsage(cpuStart);
    const rssEnd = process.memoryUsage().rss;

    await bridge.submitBrainEvent({
      wakeId,
      sessionId,
      event: { type: "finished" },
    });

    const totalMs = totalNs / 1_000_000;
    latenciesNs.sort((left, right) => left - right);

    return {
      producerBatchSize,
      events,
      textBytes,
      totalMs,
      eventsPerSecond: events / (totalMs / 1_000),
      latencyMs: {
        p50: percentile(latenciesNs, 0.5) / 1_000_000,
        p95: percentile(latenciesNs, 0.95) / 1_000_000,
        p99: percentile(latenciesNs, 0.99) / 1_000_000,
        max: latenciesNs[latenciesNs.length - 1] / 1_000_000,
      },
      rssDeltaBytes: rssEnd - rssStart,
      cpuMicros: {
        user: cpu.user,
        system: cpu.system,
        total: cpu.user + cpu.system,
      },
    };
  } finally {
    await bridge.shutdownEngine({ engine, drainTimeoutMs: 1_000 });
    rmSync(engineDataDir, { force: true, recursive: true });
  }
}

function percentile(sortedValues: number[], percentileValue: number): number {
  const index = Math.min(
    sortedValues.length - 1,
    Math.floor(sortedValues.length * percentileValue),
  );
  return sortedValues[index];
}

function parsePositiveInteger(name: string, fallback: number): number {
  const raw = process.env[name];
  if (!raw) {
    return fallback;
  }

  const parsed = Number.parseInt(raw, 10);
  if (!Number.isFinite(parsed) || parsed < 1) {
    throw new Error(`${name} must be a positive integer`);
  }

  return parsed;
}

function parseBatchSizes(raw: string): number[] {
  const parsed = raw
    .split(",")
    .map((part) => Number.parseInt(part.trim(), 10))
    .filter((value) => Number.isFinite(value) && value > 0);

  if (parsed.length === 0) {
    throw new Error("RUSTY_CREW_THROUGHPUT_BATCHES must include a batch size");
  }

  return parsed;
}
