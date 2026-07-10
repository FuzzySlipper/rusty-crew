import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const root = resolve("fixtures/runtime-config-parity/target");
const pairs = [
  ["complete-source.camel.json", "complete-source.snake.json"],
  ["complete-plan.camel.json", "complete-plan.snake.json"],
];
const check = process.argv.includes("--check");

for (const [camelName, snakeName] of pairs) {
  const camel = JSON.parse(await readFile(resolve(root, camelName), "utf8"));
  const expected = `${JSON.stringify(toSnakeCaseKeys(camel), null, 2)}\n`;
  const snakePath = resolve(root, snakeName);
  if (check) {
    assert.equal(
      await readFile(snakePath, "utf8"),
      expected,
      `${snakeName} drifted; run npm run codegen:runtime-config-target-fixtures`,
    );
  } else {
    await writeFile(snakePath, expected);
  }
}

console.log(
  check
    ? "runtime config target fixture drift check passed"
    : "runtime config target snake_case fixtures generated",
);

function toSnakeCaseKeys(value) {
  if (Array.isArray(value)) return value.map(toSnakeCaseKeys);
  if (value === null || typeof value !== "object") return value;
  return Object.fromEntries(
    Object.entries(value).map(([key, item]) => [
      key.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`),
      toSnakeCaseKeys(item),
    ]),
  );
}
