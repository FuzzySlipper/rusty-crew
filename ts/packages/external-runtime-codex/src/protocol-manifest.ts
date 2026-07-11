import { readFileSync } from "node:fs";
import type { CodexProtocolIdentity } from "./types.js";

const manifestUrl = new URL(
  "../protocol/0.144.1/manifest.json",
  import.meta.url,
);

export const CODEX_APP_SERVER_PROTOCOL = Object.freeze(
  JSON.parse(readFileSync(manifestUrl, "utf8")) as CodexProtocolIdentity,
);
