import assert from "node:assert/strict";
import test from "node:test";

import {
  classifySmoke,
  matchesFilters,
  parseArgs,
  resolveEntry,
} from "./smoke-runner.mjs";

function smoke(overrides) {
  return {
    name: "brain-island:example",
    package: "@rusty-crew/brain-island",
    script: "smoke:example",
    command: "tsx src/smoke-example.ts",
    ...overrides,
  };
}

test("classifies offline, native, service, live, and Rusty View smoke lanes", () => {
  assert.deepEqual(
    classifySmoke(smoke({ name: "brain-island:tool-registry" })),
    {
      category: "package-integration",
      lane: "offline",
      requirements: ["none"],
      tags: ["tools"],
    },
  );

  assert.equal(
    classifySmoke(smoke({ name: "native-bridge:bridge-validation" })).lane,
    "native-offline",
  );
  assert.equal(
    classifySmoke(smoke({ name: "brain-island:service-host" })).lane,
    "local-service",
  );
  assert.equal(
    classifySmoke(smoke({ name: "brain-island:direct-debug-service" })).lane,
    "debug-service",
  );
  assert.equal(
    classifySmoke(smoke({ name: "brain-island:roleplay-quality-spike-live" }))
      .lane,
    "live-provider",
  );
  assert.equal(
    classifySmoke(smoke({ name: "brain-island:rusty-view-chat-contract" }))
      .lane,
    "rusty-view-certification",
  );
});

test("parses and applies lane filters", () => {
  const args = parseArgs(["--list", "--lane", "offline", "--tag=tools"]);

  assert.equal(args.list, true);
  assert.deepEqual(args.filters, { lane: "offline", tag: "tools" });

  const entry = {
    ...smoke({ name: "brain-island:tool-registry" }),
    ...classifySmoke(smoke({ name: "brain-island:tool-registry" })),
  };
  assert.equal(matchesFilters(entry, args.filters), true);
  assert.equal(
    matchesFilters({ ...entry, lane: "live-provider" }, args.filters),
    false,
  );
});

test("resolves short smoke names only when they are unambiguous", () => {
  const entries = [
    { name: "brain-island:wake", scope: "package" },
    { name: "root-only", scope: "root-alias" },
  ];

  assert.equal(
    resolveEntry(entries, "brain-island:wake")?.name,
    "brain-island:wake",
  );
  assert.equal(resolveEntry(entries, "wake")?.name, "brain-island:wake");
  assert.equal(resolveEntry(entries, "root-only")?.name, "root-only");
});
