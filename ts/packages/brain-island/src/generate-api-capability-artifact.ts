import {
  mkdirSync,
  readFileSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { format } from "prettier";

import { apiCapabilityCoverageInventory } from "./api-capability-coverage.js";
import {
  API_CAPABILITY_OPENAPI_PATH,
  apiCapabilityOpenApiDocument,
} from "./api-capability-openapi.js";
import { apiCapabilityRegistry } from "./api-command-registry.js";
import {
  EXTERNAL_RUNTIME_API_OPENAPI_PATH,
  externalRuntimeApiOpenApiDocument,
} from "./external-runtime-api-contract.js";
import { slashCommandHandlerNames } from "./slash-command-router.js";

const capabilityOutputUrl = new URL(
  "../../../../fixtures/api-capabilities/api-command-capabilities.json",
  import.meta.url,
);
const openApiOutputUrl = new URL(
  `../../../../${API_CAPABILITY_OPENAPI_PATH}`,
  import.meta.url,
);
const externalRuntimeOpenApiOutputUrl = new URL(
  `../../../../${EXTERNAL_RUNTIME_API_OPENAPI_PATH}`,
  import.meta.url,
);
const coreProtocolSchema = JSON.parse(
  readFileSync(
    new URL(
      "../../contracts/src/generated/core-protocol.schema.json",
      import.meta.url,
    ),
    "utf8",
  ),
) as { $defs: Record<string, Record<string, unknown> | boolean> };
const capabilityArtifact = await format(
  JSON.stringify({
    schema_version: 1,
    source: "ts/packages/brain-island/src/api-command-registry.ts",
    registry: apiCapabilityRegistry(),
    route_coverage: apiCapabilityCoverageInventory(),
    slash_command_handlers: slashCommandHandlerNames(),
  }),
  { parser: "json" },
);
const openApiArtifact = await format(
  JSON.stringify(apiCapabilityOpenApiDocument()),
  { parser: "json" },
);
const externalRuntimeOpenApiArtifact = await format(
  JSON.stringify(
    externalRuntimeApiOpenApiDocument({
      coreProtocolSchemas: coreProtocolSchema.$defs,
      capabilityIds: new Set(
        apiCapabilityRegistry().capabilities.map((capability) => capability.id),
      ),
    }),
  ),
  { parser: "json" },
);
const artifacts = [
  {
    path: fileURLToPath(capabilityOutputUrl),
    content: capabilityArtifact,
  },
  { path: fileURLToPath(openApiOutputUrl), content: openApiArtifact },
  {
    path: fileURLToPath(externalRuntimeOpenApiOutputUrl),
    content: externalRuntimeOpenApiArtifact,
  },
];

if (process.argv.includes("--check")) {
  for (const artifact of artifacts) {
    const current = readFileSync(artifact.path, "utf8");
    if (current !== artifact.content) {
      throw new Error(
        `generated API capability artifact is stale: ${artifact.path}; run npm run codegen:api-capabilities`,
      );
    }
  }
  console.log("API capability and OpenAPI artifact drift check passed");
} else {
  const temporaryPaths: string[] = [];
  try {
    for (const artifact of artifacts) {
      mkdirSync(dirname(artifact.path), { recursive: true });
      const temporaryPath = `${artifact.path}.tmp`;
      writeFileSync(temporaryPath, artifact.content);
      temporaryPaths.push(temporaryPath);
    }
    for (const [index, artifact] of artifacts.entries()) {
      renameSync(temporaryPaths[index]!, artifact.path);
      console.log(`wrote ${artifact.path}`);
    }
  } finally {
    for (const temporaryPath of temporaryPaths) {
      rmSync(temporaryPath, { force: true });
    }
  }
}
