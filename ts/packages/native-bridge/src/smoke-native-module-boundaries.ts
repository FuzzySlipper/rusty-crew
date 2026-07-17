import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const moduleNames = [
  "admin-wrappers.ts",
  "brain-run-wire.ts",
  "brain-wrappers.ts",
  "chat-wrappers.ts",
  "curator-wrappers.ts",
  "event-body-wire.ts",
  "external-runtime-public-api.ts",
  "external-runtime-turn-wrappers.ts",
  "external-runtime-wrappers.ts",
  "index.ts",
  "memory-wrappers.ts",
  "model-provider-public-api.ts",
  "profile-provider-wire.ts",
  "profile-provider-wrappers.ts",
  "public-api.ts",
  "roleplay-proposal-wrappers.ts",
  "roleplay-mechanic-wrappers.ts",
  "roleplay-wrappers.ts",
  "runtime-config-wire.ts",
  "runtime-config-wrappers.ts",
  "scheduler-wrappers.ts",
  "service-credential-wire.ts",
  "service-credential-wrappers.ts",
  "session-wire.ts",
] as const;

const sourceDirectory = fileURLToPath(new URL("./", import.meta.url));
const sources = new Map<string, string>(
  moduleNames.map((name) => [
    name,
    readFileSync(`${sourceDirectory}/${name}`, "utf8"),
  ]),
);
const lineCeilings: Record<string, number> = {
  "admin-wrappers.ts": 120,
  "brain-run-wire.ts": 750,
  "brain-wrappers.ts": 40,
  "chat-wrappers.ts": 340,
  "curator-wrappers.ts": 100,
  "event-body-wire.ts": 850,
  "external-runtime-public-api.ts": 130,
  "external-runtime-turn-wrappers.ts": 60,
  "external-runtime-wrappers.ts": 180,
  "index.ts": 1_900,
  "memory-wrappers.ts": 250,
  "model-provider-public-api.ts": 170,
  "profile-provider-wire.ts": 650,
  "profile-provider-wrappers.ts": 210,
  "public-api.ts": 2_550,
  "roleplay-proposal-wrappers.ts": 50,
  "roleplay-mechanic-wrappers.ts": 70,
  "roleplay-wrappers.ts": 330,
  "runtime-config-wire.ts": 900,
  "runtime-config-wrappers.ts": 220,
  "scheduler-wrappers.ts": 250,
  "service-credential-wire.ts": 160,
  "service-credential-wrappers.ts": 130,
  "session-wire.ts": 100,
};
const HANDWRITTEN_CONVERTER_CEILING = 91;

for (const [name, source] of sources) {
  const lines = source.split("\n").length;
  assert(
    lines <= lineCeilings[name],
    `${name} grew to ${lines} lines; split its family before raising the ${lineCeilings[name]} line ceiling`,
  );
  if (name !== "index.ts") {
    assert(
      !source.includes('from "./index.js"'),
      `${name} must depend on public-api or focused wire modules, not the composition entrypoint`,
    );
  }
}

const graph = new Map<string, string[]>();
for (const [name, source] of sources) {
  const dependencies = [...source.matchAll(/from "\.\/([^".]+)\.js"/g)]
    .map((match) => `${match[1]}.ts`)
    .filter((dependency) => sources.has(dependency));
  graph.set(name, [...new Set(dependencies)]);
}

const visited = new Set<string>();
const active = new Set<string>();
function visit(name: string, path: string[]): void {
  if (active.has(name)) {
    throw new Error(
      `native bridge module cycle: ${[...path, name].join(" -> ")}`,
    );
  }
  if (visited.has(name)) return;
  active.add(name);
  for (const dependency of graph.get(name) ?? []) {
    visit(dependency, [...path, name]);
  }
  active.delete(name);
  visited.add(name);
}
for (const name of graph.keys()) visit(name, []);

const handwrittenConverterCount = [...sources.values()].reduce(
  (count, source) =>
    count +
    [...source.matchAll(/^(?:export )?function (?:to|from)[A-Z]/gm)].length,
  0,
);
assert(
  handwrittenConverterCount <= HANDWRITTEN_CONVERTER_CEILING,
  `handwritten native bridge converter count grew to ${handwrittenConverterCount}; generate or consolidate mappings before raising the ${HANDWRITTEN_CONVERTER_CEILING} ceiling`,
);

console.log(
  JSON.stringify({
    modules: moduleNames.length,
    indexLines: sources.get("index.ts")?.split("\n").length,
    runtimeImportCycles: 0,
    entrypointBackImports: 0,
    handwrittenConverterCount,
    handwrittenConverterCeiling: HANDWRITTEN_CONVERTER_CEILING,
  }),
);
