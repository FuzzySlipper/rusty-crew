import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  TELEGRAM_DIPLOMAT_ADMIN_OPENAPI_PATH,
  TELEGRAM_DIPLOMAT_ADMIN_PATHS,
  TELEGRAM_DIPLOMAT_STATE_VALUES,
} from "../src/telegram-diplomat-admin-contract.js";

interface OpenApiSchema {
  enum?: string[];
  required?: string[];
  properties?: Record<string, unknown>;
}

interface OpenApiDocument {
  openapi: string;
  paths: Record<string, unknown>;
  components: { schemas: Record<string, OpenApiSchema> };
}

const contract = JSON.parse(
  readFileSync(
    resolve(process.cwd(), "../../../", TELEGRAM_DIPLOMAT_ADMIN_OPENAPI_PATH),
    "utf8",
  ),
) as OpenApiDocument;

assert.equal(contract.openapi, "3.1.0");
for (const path of Object.values(TELEGRAM_DIPLOMAT_ADMIN_PATHS)) {
  assert.ok(contract.paths[path], `missing Telegram diplomat path ${path}`);
}
assert.deepEqual(contract.components.schemas.TelegramDiplomatState?.enum, [
  ...TELEGRAM_DIPLOMAT_STATE_VALUES,
]);
assert.deepEqual(
  contract.components.schemas.TelegramDiplomatReadback?.required,
  ["state", "enabled", "adapterId", "credentialId", "candidates", "bindings"],
);
for (const diagnostic of ["pollCount", "inbound", "outbound", "media"]) {
  assert.ok(
    contract.components.schemas.TelegramConnectorDiagnostics?.properties?.[
      diagnostic
    ],
    `missing connector diagnostic ${diagnostic}`,
  );
}

console.log("telegram diplomat admin contract smoke ok");
