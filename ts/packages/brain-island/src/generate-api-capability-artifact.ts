import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { format } from "prettier";

import { apiCapabilityCoverageInventory } from "./api-capability-coverage.js";
import { apiCapabilityRegistry } from "./api-command-registry.js";
import { slashCommandHandlerNames } from "./slash-command-router.js";

const outputUrl = new URL(
  "../../../../fixtures/api-capabilities/api-command-capabilities.json",
  import.meta.url,
);
const outputPath = fileURLToPath(outputUrl);
const artifact = await format(
  JSON.stringify({
    schema_version: 1,
    source: "ts/packages/brain-island/src/api-command-registry.ts",
    registry: apiCapabilityRegistry(),
    route_coverage: apiCapabilityCoverageInventory(),
    slash_command_handlers: slashCommandHandlerNames(),
  }),
  { parser: "json" },
);

if (process.argv.includes("--check")) {
  const current = readFileSync(outputPath, "utf8");
  if (current !== artifact) {
    throw new Error(
      `generated API capability artifact is stale: ${outputPath}; run npm run codegen:api-capabilities`,
    );
  }
  console.log("API capability generated artifact drift check passed");
} else {
  mkdirSync(fileURLToPath(new URL("./", outputUrl)), { recursive: true });
  writeFileSync(outputPath, artifact);
  console.log(`wrote ${outputPath}`);
}
