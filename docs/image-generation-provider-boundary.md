# Image Generation Provider Boundary

Rusty Crew exposes one neutral `image_generate` capability. A profile receives
it only through the normal `image_generation` local toolset. The model can
choose a configured preset and bounded prompt controls; it cannot submit a
provider URL, workflow graph, node mapping, output path, or batch size.

## Runtime Configuration

Image generation is configured under `imageGeneration` in the service's
`config/service.json`. Configuration reload rebuilds brain tool surfaces, so a
service binary or frontend rebuild is not required.

```json
{
  "imageGeneration": {
    "providers": [
      {
        "id": "comfy-local",
        "kind": "comfyui",
        "endpointUrl": "http://127.0.0.1:8188",
        "requestTimeoutMs": 30000,
        "generationTimeoutMs": 300000,
        "pollIntervalMs": 500,
        "bearerTokenEnv": "COMFYUI_BEARER_TOKEN"
      }
    ],
    "presets": [
      {
        "id": "portrait-v1",
        "version": "1.0.0",
        "providerId": "comfy-local",
        "workflow": {
          "6": { "class_type": "CLIPTextEncode", "inputs": { "text": "" } },
          "3": {
            "class_type": "KSampler",
            "inputs": { "seed": 0, "steps": 20 }
          },
          "5": {
            "class_type": "EmptyLatentImage",
            "inputs": { "width": 1024, "height": 1024 }
          }
        },
        "inputs": {
          "prompt": { "nodeId": "6", "inputName": "text" },
          "seed": { "nodeId": "3", "inputName": "seed" },
          "steps": { "nodeId": "3", "inputName": "steps" },
          "width": { "nodeId": "5", "inputName": "width" },
          "height": { "nodeId": "5", "inputName": "height" }
        },
        "defaults": { "width": 1024, "height": 1024, "steps": 20 },
        "limits": {
          "minWidth": 512,
          "maxWidth": 1536,
          "minHeight": 512,
          "maxHeight": 1536,
          "minSteps": 1,
          "maxSteps": 50,
          "maxPromptChars": 8000,
          "maxOutputs": 1
        },
        "outputNodeIds": ["9"]
      }
    ]
  }
}
```

Workflow JSON must be ComfyUI API format, not the browser workflow export.
Every exposed input maps to an existing workflow node input. Presets are
versioned because changing a graph or mapping changes generation provenance.
ComfyUI presets must map seed, width, height, and steps so recorded provenance
cannot diverge from the submitted graph; negative prompt and style remain
optional.

## Provider Contract

An image provider receives a normalized request containing preset identity,
prompt, optional negative prompt and style, seed, dimensions, and steps. It
reports queued/running/completed/failed/cancelled status and returns typed image
content. The tool host then persists those bytes through Crew's durable
attachment path; providers do not write transcript records or public URLs.

The ComfyUI implementation uses only configured `/prompt`, `/queue`,
`/history/{prompt_id}`, and `/view` paths. Cancellation removes the queued job;
global `/interrupt` is disabled unless an operator explicitly enables
`allowGlobalInterrupt` for that provider.

A later MCP or non-Comfy adapter should implement `ImageGenerationProvider`.
It must not add provider-specific fields to the model tool schema or bypass
attachment persistence.

## Operator API

- `GET /v1/admin/image-generation/presets` returns a redacted preset catalog.
- `POST /v1/admin/image-generation/generate` accepts `session_id` plus the same
  bounded tool fields and returns durable attachment references.

Both routes use normal service authorization. Public responses and chat events
never contain workflow JSON, credentials, endpoint URLs, or base64 image data.

## Live Test

The deterministic fake-provider smoke is part of offline verification. A real
ComfyUI deployment is exercised only when `RUSTY_CREW_COMFYUI_LIVE=1` and a
live test configuration file is supplied explicitly; normal CI never reaches a
configured image provider.

```bash
RUSTY_CREW_COMFYUI_LIVE=1 \
RUSTY_CREW_COMFYUI_CONFIG_PATH=/path/to/service.json \
npm --workspace @rusty-crew/brain-island run smoke:image-generation-live
```
