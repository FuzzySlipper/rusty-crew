import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  createImageGenerationRuntime,
  imageGenerationConfigFromUnknown,
  imageGenerationTool,
} from "../src/image-generation.js";

if (process.env.RUSTY_CREW_COMFYUI_LIVE !== "1") {
  console.log(
    "image-generation-live: skipped (set RUSTY_CREW_COMFYUI_LIVE=1 and RUSTY_CREW_COMFYUI_CONFIG_PATH)",
  );
  process.exit(0);
}

const configPath = process.env.RUSTY_CREW_COMFYUI_CONFIG_PATH;
assert.ok(
  configPath,
  "RUSTY_CREW_COMFYUI_CONFIG_PATH must name a JSON file containing imageGeneration config",
);
const source = JSON.parse(await readFile(configPath, "utf8")) as {
  imageGeneration?: unknown;
};
const config = imageGenerationConfigFromUnknown(
  source.imageGeneration ?? source,
);
const preset = process.env.RUSTY_CREW_COMFYUI_PRESET ?? config.presets[0]?.id;
assert.ok(preset, "the live config must contain at least one workflow preset");
const result = await imageGenerationTool(
  createImageGenerationRuntime(config),
).execute("live-image-generation", {
  preset,
  prompt:
    process.env.RUSTY_CREW_COMFYUI_PROMPT ??
    "A small brass compass on a clean white background",
  ...(process.env.RUSTY_CREW_COMFYUI_SEED
    ? { seed: Number(process.env.RUSTY_CREW_COMFYUI_SEED) }
    : {}),
});
assert.equal((result.details as { ok?: boolean }).ok, true);
assert.ok(
  result.content.some((item) => item.type === "image" && item.data.length > 0),
  "live ComfyUI run did not return typed image media",
);
console.log(
  JSON.stringify({
    ok: true,
    preset,
    image_count: result.content.filter((item) => item.type === "image").length,
    provenance: (result.details as { provenance?: unknown }).provenance,
  }),
);
