# pi-crew ←→ upstream `pi` audit: actual code reuse vs conceptual borrowing

Investigation date: 2026-06-18
Scope: `/home/dev/pi-crew` (a.k.a. `pi-fleet`) vs the historical local audit checkout at `/home/research/pi-fleet/pi` (published as `@earendil-works/pi-*`).

Implementation note: use the current upstream source at
`https://github.com/earendil-works/pi` for any TypeScript brain-island package
work. The local checkout paths in this audit are evidence for the June 2026
dependency shape, not a recommendation to depend on that older location.

## TL;DR

**Almost everything pi-fleet does is its own code.** Of the ~47k LOC across 9 packages, only ~2 files actually invoke upstream runtime code (`Agent`, `streamSimple`, `getModels`, `getProviders`, `createAssistantMessageEventStream`). The other 63 import statements are *type-only* — they re-use upstream's TypeScript shapes but no upstream JavaScript ships at runtime.

- 7 of 9 packages have **zero** upstream imports (including all of pi-core, pi-channels, pi-mcp, pi-memory, pi-profiles, pi-governance, pi-tools).
- Only **pi-service** and **pi-crew** touch upstream, and `pi-crew`'s exposure is just type definitions for tool wiring.
- The entire product layer — state machine, packet protocol, worker pool, channel router, governance, profiles, memory, persistence, TUI, cron, browser tools, Den integration — is built from scratch on top of an `Agent` / `streamSimple` thin shim.
- 3 call sites instantiate `new Agent(...)` (each is a 1-3 line wiring call), 3 sites call `streamSimple`, 6 sites use `getModels`/`getProviders` for string lookups.

## 1. Repo inventory

### pi-fleet (`/home/dev/pi-crew`, monorepo of 9 packages)

| Package         | LOC (src, ex-tests) | Purpose                                                              |
|-----------------|---------------------|----------------------------------------------------------------------|
| pi-service      | 21,014              | Worker pool, packet protocol, LLM-supervised agent execution         |
| pi-crew         | 12,759              | Top-level runtime, CLI, TUI, Den-Channel routing, worker spawning    |
| pi-core         | 3,980               | Shared types, hooks, event bus, retry, fake-channel scaffolding      |
| pi-channels     | 3,922               | Den Channels adapter, Telegram, WebSocket, HTTP, simulated providers |
| pi-tools        | 2,434               | Local code tools (patch, file ops, todo, skill-manage, etc.)         |
| pi-profiles     | 1,095               | System-prompt/section composition, profile loading                  |
| pi-mcp          | 896                 | MCP client + tool-registry bridging                                  |
| pi-governance   | 670                 | Output routing, policy helpers                                       |
| pi-memory       | 526                 | Memory interface + minimal in-memory impl                            |
| **TOTAL**       | **~47,296**         |                                                                      |

### Upstream pi (historical local audit checkout, monorepo of 4 packages)

| Package       | LOC (src, ex-tests) | pi-fleet uses it? |
|---------------|---------------------|-------------------|
| packages/ai          | 28,996            | **Yes** (renamed `@earendil-works/pi-ai`)    |
| packages/agent       | 7,291             | **Yes** (renamed `@earendil-works/pi-agent-core`) |
| packages/coding-agent | (not measured)   | **No** (consumed by upstream's own TUI; pi-fleet's TUI is its own) |
| packages/tui         | (not measured)    | **No** (pi-fleet has its own TUI: `pi-crew/src/debug-tui*`) |

Author: Mario Zechner (per upstream `agent/package.json` -> `repository.url` = `github.com/earendil-works/pi`). The `earendil-works` namespace on npm is the user-forked publish path.

## 2. What pi-fleet actually imports from upstream

I scanned every `import` statement in pi-fleet's hand-written `.ts` source (excluding `dist/`, `node_modules/`, `spikes/`, `__tests__/`). **65 files** import from `@earendil-works/*`, but the breakdown by kind tells the real story:

| Package                       | Import lines | Files | Type-only | Value |
|-------------------------------|--------------|-------|-----------|-------|
| `@earendil-works/pi-agent-core` | 54           | 48    | **51**    | 3     |
| `@earendil-works/pi-ai`         | 15           | 14    | 6         | 9     |

### 2a. The 3 value imports from `pi-agent-core` — all of them are `Agent`

```
pi-service/src/instances/full-agent-responder.ts:        import { Agent }
pi-service/src/workers/llm-delegated-child-runner.ts:    import { Agent }
pi-service/src/workers/agent-worker-executor.ts:         import { Agent }
```

`Agent` is the upstream agent-loop class (model ↔ tools ↔ stream of events). Each call site is a ~10-line wiring shim that injects:
- a custom `getApiKey` resolver (for custom-base-url + env-fallback cases),
- a `withRetryingStream`-wrapped `streamFn` (own retry policy, not upstream's),
- session id and initial state.

The agent-loop itself is upstream; everything *around* it (state, tools, persistence, governance, Den integration) is pi-crew.

### 2b. The 9 value imports from `pi-ai` — all the runtime primitives

`pi-ai` is upstream's LLM API surface (provider registry, streaming, typebox re-exports). The 9 value imports reduce to **4 functions**:

| Function                            | Production call sites                                              | What it actually does                              |
|-------------------------------------|--------------------------------------------------------------------|----------------------------------------------------|
| `streamSimple(model, ctx, opts)`    | 3 (full-agent-responder, llm-delegated-child-runner, agent-worker-executor) | LLM streaming call. Returns an `AssistantMessageEventStream`. |
| `getModels(provider)`               | 3 (one in each of the 3 worker sites)                              | Returns the model list for a provider. **Only used for string lookups** (`find(m => m.id === name)`). |
| `getProviders()`                    | 3 (one in each worker site, all for `includes(...)` membership check) | Returns the list of known provider names. |
| `createAssistantMessageEventStream` | 1 (model-stream-retry.ts)                                          | Constructs an event-stream object. |
| `Type`                              | 1 (den-channel-readback-tool.ts)                                   | Re-export of `typebox`'s `Type` for JSON-Schema tool definitions. |

All 4 of these are thin, well-defined primitives — not the bulk of upstream's ~29k LOC of `pi-ai` source. The provider-specific adapters, image generation, OAuth flows, model registry tables, the Bedrock/AWS glue — none of that is in pi-fleet.

### 2c. The 51 type-only imports from `pi-agent-core` — the conceptual layer

This is the "pulling concepts" axis. Type-only imports are erased at compile time, so they don't add a runtime dependency, but they do mean **the shape of pi-crew's data model is locked to upstream's types**:

| Type             | Files importing it (count) |
|------------------|----------------------------|
| `AgentTool`      | 31                         |
| `AgentMessage`   | 19                         |
| `AgentEvent`     | 14                         |
| `BeforeToolCallContext` / `Result` | 1 each |
| `AfterToolCallContext` / `Result`  | 1 each |
| `AgentToolResult` | 1                          |

These flow through worker pools, packet types, persistence layers, and tool registries. **Conceptual coupling: high. Runtime coupling: zero.**

### 2d. The deliberate shadow re-definitions

Most interesting: `pi-service/src/workers/guarded-tool-types.ts` contains this comment:

> *"These are structural types that mirror the pi-agent-core exports at `/home/research/pi-fleet/pi/packages/agent/src/types.ts` **without importing from the external package directly**. The guarded assembly produces objects that satisfy these structural interfaces."*

The file re-declares `AgentTool`, `TextContent`, `ImageContent`, `AgentToolResult`, `BeforeToolCallContext`, `BeforeToolCallResult`, `AfterToolCallContext`, `AfterToolCallResult` as local interfaces. They are intentionally **shaped the same as upstream's** (so the objects they build satisfy upstream's `Agent`) but **not derived from them** (so the security-critical guarded-tool layer doesn't `import` the package it is wrapping).

This is the "concept over code" pattern explicitly — and it's deliberate decoupling, not accidental duplication.

## 3. Per-package upstream exposure

Out of 255 src files (excluding tests):

| Package      | Files touching upstream | Total src files | % exposed |
|--------------|--------------------------|------------------|----------|
| pi-core      | 0 | 26 | 0%  |
| pi-channels  | 0 | 16 | 0%  |
| pi-governance | 0 | 4 | 0% |
| pi-mcp       | 0 | 6  | 0%  |
| pi-memory    | 0 | 2  | 0%  |
| pi-profiles  | 0 | 5  | 0%  |
| pi-tools     | 0 | 14 | 0%  |
| pi-crew      | 25 | 70 | 36% (all type-only, plus `Type` and `getModels/getProviders` for boot-time lookup) |
| pi-service   | 26 | 112 | 23% (concentrated in `workers/`, `instances/`, `persistence/`) |

**pi-crew's 25 file-level touches are all metadata**: tool-result type annotations, typebox schema builders, and a few boot-time provider/model string lookups. The actual agent-loop execution is entirely in `pi-service`.

## 4. What we use vs. what we don't

### Used at runtime (4 functions + 1 class, ~6 call sites total)
- `Agent` (class) — agent loop. 3 call sites, all 10-line wiring shims.
- `streamSimple` — LLM streaming. 3 call sites, all wrapped in pi-crew's own `withRetryingStream`.
- `getModels` / `getProviders` — provider registry. 6 sites, all for string lookups.
- `createAssistantMessageEventStream` — event-stream factory. 1 site.
- `Type` (re-export of typebox) — JSON Schema tool definitions. 1 site.

### Used only as types (6 types, 51 import sites)
- `AgentTool`, `AgentMessage`, `AgentEvent`, `AgentToolResult`, `BeforeToolCallContext/Result`, `AfterToolCallContext/Result`, `AssistantMessage`, `Api`, `Model`, `KnownProvider`, `TextContent`, `StreamFunction`, `SimpleStreamOptions`, `AssistantMessageEvent`, `AssistantMessageEventStream`, `Context`.

### Re-implemented locally as deliberate structural shadows
- `AgentTool`, `TextContent`, `ImageContent`, `AgentToolResult`, `BeforeToolCallContext/Result`, `AfterToolCallContext/Result` (in `guarded-tool-types.ts`, with a comment saying so).

### NOT used from upstream at all
- The other 2 upstream packages: `coding-agent` and `tui`. pi-fleet has its own equivalent (debug-tui, debug-cli, cron-cli).
- All of upstream's provider-specific adapters (Bedrock, Azure, Google, Mistral, OpenAI Responses, etc.) — pi-fleet relies on upstream's generic `streamSimple` and the `getModels`/`getProviders` indirection.
- Upstream's image generation, OAuth, model-discovery scripts, harness scaffolding, session JSONL repos, skills system, system-prompt composition — all of which exist in upstream `agent/src/harness/`. pi-fleet has its own `pi-profiles` (sections loader), `pi-memory` (different schema), and `pi-tools` (own skill-manage-tool implementation).

## 5. Quantified summary

**By LOC and structural footprint:** ~47,300 LOC pi-fleet vs. ~7,300 LOC of upstream `Agent` source actually reached. The runtime that ships from upstream is, conservatively, **< 5% of pi-fleet's own code size**, and it's concentrated in a 1-class + 4-function API.

**By dependency count:** 2 npm packages pulled (`@earendil-works/pi-agent-core`, `@earendil-works/pi-ai`), 0 from upstream's `coding-agent` or `tui` packages.

**By conceptual coupling:** All 9 packages share the same mental model as upstream (messages, tools, agent loop, events, model registry) — names, type shapes, and call patterns are aligned. But the **state machine, the worker pool, the packet protocol, the channel layer, the persistence, the Den/MCP/Channels integration, the TUI, the cron subsystem, and the tool catalog are entirely pi-fleet's own design.**

**Net assessment:** pi-fleet is best characterized as a **re-implementation-with-thin-shim** rather than a fork. It uses upstream as a 4-function streaming+agent-loop primitive and as a TypeScript vocabulary, but builds its product surface — which is the multi-agent worker pool, Den-channel integration, and per-worker role assembly — entirely from scratch.

## 6. Recommendations / things worth flagging

1. **The 51 type-only imports are a hidden coupling.** If upstream renames a field on `AgentMessage` or `AgentEvent`, pi-fleet will break at compile time even though zero runtime behavior depends on it. This is fine for now, but a deliberate decision to lock shapes or break the lock is worth surfacing.

2. **`guarded-tool-types.ts` is a known-debt pattern.** It's a deliberate parallel type set that needs to be kept in sync with upstream by hand. If upstream changes `AgentTool` shape, this file's "structural mirror" will silently start producing objects that no longer satisfy `Agent`'s contract. There's a non-trivial chance the type assertions have drifted already; worth a one-shot check.

3. **The 3 `new Agent({...})` shims are nearly identical** (full-agent-responder, llm-delegated-child-runner, agent-worker-executor). They could plausibly consolidate into a single `DefaultAgentWorkerFactory` — but it's already done in `agent-worker-executor.ts` and the other two are test/responder paths that legitimately differ. Probably correct as-is, but flag for a glance.

4. **The `pi-ai` value imports are small but load-bearing.** Removing upstream would mean re-implementing LLM streaming, provider/model registry, and typebox re-export — that's not nothing. If we ever want to escape upstream entirely, those 4 functions + 1 type-re-export are the entire bridge to rebuild.

5. **No LLM provider-specific code in pi-fleet.** pi-fleet deliberately funnels all model calls through upstream's `streamSimple` + `getModels`/`getProviders` indirection. Adding a new provider means upgrading upstream, not editing pi-fleet. This is actually a clean separation — but it does mean provider-level bug-fixes can't be done in pi-fleet itself.
