import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const root = resolve(import.meta.dirname, "..");
const read = (relativePath) =>
  readFileSync(resolve(root, relativePath), "utf8");
const matrixPath = "fixtures/context-accounting/compaction-matrix.json";
const matrix = JSON.parse(read(matrixPath));

assert.equal(matrix.schemaVersion, 1);
assert.ok(Array.isArray(matrix.cases) && matrix.cases.length >= 16);

const allowedSourceQualities = new Set([
  "provider/exact",
  "tokenizer/approximate",
  "serialized_estimate/approximate",
  "unavailable/unavailable",
]);
const allowedFixtureKinds = new Set([
  "diagnostic",
  "decision",
  "event_order",
  "lineage",
  "persistence",
  "schema",
  "snapshot",
]);
const productionProbePackages = new Set([
  "rusty-crew-brain-runtime",
  "rusty-crew-chat-completions-brain",
  "rusty-crew-core-persistence",
  "rusty-crew-openai-responses-brain",
]);
let productionProbeCount = 0;

for (const fixtureCase of matrix.cases) {
  assert.match(fixtureCase.id, /^[a-z0-9-]+$/);
  assert.ok(allowedSourceQualities.has(fixtureCase.sourceQuality));
  assert.ok(fixtureCase.fixture && typeof fixtureCase.fixture === "object");
  assert.ok(allowedFixtureKinds.has(fixtureCase.fixture.kind));
  assert.ok(
    Array.isArray(fixtureCase.testRefs) && fixtureCase.testRefs.length > 0,
    `${fixtureCase.id} must identify executable regression coverage`,
  );

  if (fixtureCase.fixture.path) {
    const fixturePath = `fixtures/context-accounting/${fixtureCase.fixture.path}`;
    assert.ok(existsSync(resolve(root, fixturePath)), `${fixturePath} missing`);
  }
  if (fixtureCase.fixture.fingerprint) {
    const fingerprintPath = `fixtures/context-accounting/${fixtureCase.fixture.fingerprint}`;
    assert.ok(
      existsSync(resolve(root, fingerprintPath)),
      `${fingerprintPath} missing`,
    );
  }

  for (const testRef of fixtureCase.testRefs) {
    const separator = testRef.indexOf("::");
    assert.ok(separator > 0, `${fixtureCase.id} has malformed test ref`);
    const file = testRef.slice(0, separator);
    const functionName = testRef.slice(separator + 2);
    assert.ok(existsSync(resolve(root, file)), `${testRef} file missing`);
    assert.match(
      read(file),
      new RegExp(`(?:fn|function)\\s+${functionName}\\b`),
      `${testRef} is not present in the source tree`,
    );
  }

  if (fixtureCase.productionProbes !== undefined) {
    assert.ok(
      Array.isArray(fixtureCase.productionProbes) &&
        fixtureCase.productionProbes.length > 0,
      `${fixtureCase.id} production probes must be non-empty`,
    );
    for (const probe of fixtureCase.productionProbes) {
      assert.ok(probe && typeof probe === "object");
      assert.ok(productionProbePackages.has(probe.package));
      assert.match(probe.filter, /^[A-Za-z0-9_:]+$/);
      assert.ok(
        fixtureCase.testRefs.some(
          (testRef) =>
            testRef.slice(testRef.indexOf("::") + 2) === probe.filter,
        ),
        `${fixtureCase.id} production probe must match a declared test ref`,
      );
      const result = spawnSync(
        "cargo",
        [
          "test",
          "-p",
          probe.package,
          "--lib",
          probe.filter,
          "--",
          "--nocapture",
        ],
        {
          cwd: root,
          encoding: "utf8",
          stdio: "pipe",
        },
      );
      assert.equal(
        result.status,
        0,
        `${fixtureCase.id} production probe failed: cargo test -p ${probe.package} --lib ${probe.filter} -- --nocapture\n${result.stdout}\n${result.stderr}`,
      );
      assert.match(
        result.stdout,
        /running [1-9][0-9]* test/,
        `${fixtureCase.id} production probe did not run a test`,
      );
      productionProbeCount += 1;
    }
  }
}

console.log(
  `context accounting fixture catalog passed (${matrix.cases.length} executable cases; ${productionProbeCount} production probes)`,
);
