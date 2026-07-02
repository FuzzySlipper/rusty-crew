# Rust LLM / agent ecosystem — research notes for the pi-crew core rewrite

> **For the implementing agent.** Online research scout (2026-06) of the Rust crates landscape around the rewrite. Goal: identify libraries we could lean on to avoid re-inventing LLM plumbing, agent loops, tool calling, MCP, structured output, and provider abstraction. **Not** a recommendation to vendor a giant framework — most of these are too large / too opinionated for what pi-crew needs. The list is curated to *what actually fits the manifest's "Rust owns authority, TS owns expression" architecture.*
>
> **Pre-read:** `pi-crew-core-bridge-manifest.md` (especially §"LLM boundary" — TS-side LLM, Rust-side authority). This doc assumes the LLM call stays TS-side for v1; the libraries below are mostly relevant for *future* scope (Tier 2 of the manifest) or for adjacent concerns (tool schema, MCP, structured output) that may end up in Rust regardless.

## TL;DR — what to actually consider

| Concern | Primary candidate | Runner-up | Verdict |
|---------|------------------|-----------|---------|
| **Multi-provider LLM client (future Tier 2)** | [`rig-core`](https://github.com/0xPlaygrounds/rig) | [`genai`](https://github.com/jeremychone/rust-genai) | Rig if you want the Agent/tool/embedding ecosystem; genai if you want protocol purity and minimalism. Both irrelevant for v1 (LLM is TS-side). |
| **Anthropic-specific client** | [`anthropic-ai-sdk`](https://crates.io/crates/anthropic-ai-sdk) or [`bosun-ai/async-anthropic`](https://github.com/bosun-ai/async-anthropic) | — | Only relevant if/when we bring LLM into Rust. Use the typed-error `ApiError` enum pattern from Claw Code as a shape reference. |
| **OpenAI / OpenAI-compatible client** | [`64bit/async-openai`](https://github.com/64bit/async-openai) | [`openai_dive`](https://crates.io/crates/openai_dive) | Same — future scope only. async-openai's `BYOT` (bring-your-own-types) feature is interesting if you want to abstract over compatible providers. |
| **Tool / function-calling abstraction** | Rig's `Tool` trait | — | Worth borrowing the *shape* (associated `NAME: &'static str`, `Args`, `Output`, `Error` types + `definition()` for JSON Schema + `call()`) even if you don't pull in Rig itself. Maps onto the manifest's `ToolExecutorDescriptor` directly. |
| **JSON Schema generation from Rust types** | [`schemars`](https://docs.rs/schemars) (re-exported by Rig) | — | Use this if any Rust tool registry wants to ship schema. It's a `#[derive(JsonSchema)]` macro that turns Rust structs into JSON Schema, avoiding the `serde_json::json!({"type": "object", "properties": {...}})` hand-typing pattern pi-crew currently has in TS. |
| **MCP (Model Context Protocol) client/server** | [`rust-mcp-sdk`](https://crates.io/crates/rust-mcp-sdk) (149K downloads, maintained) | `modelcontextprotocol` official SDK | **Pick this if/when MCP moves into Rust.** For now MCP is TS-side (`pi-mcp/`), so defer. When it moves, the `rust-mcp-sdk` is the consensus choice — has its own schema crate, examples, and active maintenance. |
| **Structured output (typed LLM responses)** | [`rstructor`](https://crates.io/crates/rstructor) ("Rust Instructor") | [`instructor-rs`](https://github.com/instructor-ai/instructor-rs) | Only relevant if Rust ever produces structured outputs (e.g. for `CompletionPacket`). rstructor is the cleaner of the two and has its own `Instructor` derive macro. |
| **Architecture reference for a multi-agent Rust system** | [Claw Code Rust runtime](https://claw-code.codes/rust-runtime) | — | **This is the closest prior art to what pi-crew's manifest is building.** 6-crate workspace, ~20K LOC total, does agent loop + tool execution + permission enforcement + streaming. Read this *before* writing the workspace `Cargo.toml`. |
| **Anti-references** | `echo-agent`, `mini-agent`, `agent-runtime`, `agent_sdk` | — | Too opinionated, too much surface, or too immature. Skip. |

---

## Tier 1 — Directly useful for the rewrite

### Rig (`rig-core`) — primary candidate for future LLM/Rust work

- **Repo:** https://github.com/0xPlaygrounds/rig
- **Crate:** `rig-core` (7K+ stars, 173+ dependent repos, v0.39.0)
- **Maintainer:** Playgrounds / Ryzome (commercial, but the crate is MIT)
- **Provider coverage:** OpenAI, Anthropic, Gemini, Cohere, AWS Bedrock, Groq — natively. 20+ vector-store integrations as separate companion crates (don't pull those in).
- **Public API:** `CompletionClient` / `ProviderClient` / `CompletionModel` / `EmbeddingModel` traits + an `Agent` builder.

**Why it's interesting for pi-crew:**

1. **The `Tool` trait shape is exactly what the manifest's `ToolExecutorDescriptor` wants.** From the docs (https://docs.rig.rs/docs/concepts/tools):
   ```rust
   impl Tool for Adder {
       const NAME: &'static str = "add";
       type Error = MathError;
       type Args = AddArgs;
       type Output = i32;
       async fn definition(&self, _prompt: String) -> ToolDefinition { ... }
       async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> { ... }
   }
   ```
   The associated-types pattern (`NAME`, `Args`, `Output`, `Error`) gives you typed tool dispatch without `Box<dyn Any>` — which is exactly what the manifest's "operations are statically dispatched" rule needs. **Steal this trait shape even if you don't depend on Rig.** It encodes "every tool is a unit struct with a name, an arg type, an output type, and an error type" in the type system.

2. **`CompletionModel` is a trait, not a concrete type.** Custom providers just implement the trait. The agent loop doesn't care which provider is plugged in. This is the same dispatch-table-by-trait-object pattern the manifest wants for `WorkerHandle` → `ToolExecutorDescriptor` lookup, and for `register_model_catalog`.

3. **`schemars::JsonSchema` derive integration** for `definition()`. The migration note in the docs (rig-core v0.31.0+) is informative:
   > `#[schemars(description = "...")]` attribute is replaced by `#[doc]` comments (standard `///` doc comments) for field descriptions
   This is exactly the convention the manifest's codegen should adopt: derive JSON Schema from struct doc-comments, no hand-written schema literals.

4. **Tool servers (added in v0.22.0)** — separate Tokio-spawned tasks that handle tools via message-passing instead of `Arc<Mutex<>>`. This is the *correct* way to do concurrent tool dispatch in async Rust; the `Arc<Mutex>` pattern they explicitly call out as the problem is exactly what some pi-crew TS code does via shared mutable references.

**What to ignore:**
- The full `Agent` builder + memory + RAG stack — pi-crew has its own orchestrator, own packet protocol, own memory (`pi-memory/`).
- The 20+ vector-store companion crates — pi-crew's RAG story is different.
- The warning at the top of their README: *"future updates **will** contain **breaking changes**."* The pre-1.0 churn risk means we should not `cargo add rig-core` for the rewrite; we should either pin to a specific version or, more likely, borrow the trait shapes as design references.

**Net:** Rig is *the* reference for "what does idiomatic Rust LLM tooling look like in 2026." Worth reading the `Tool` trait docs and the `CompletionModel` trait signature. Not worth depending on for v1.

### Claw Code's 6-crate Rust workspace — architecture prior art

- **Page:** https://claw-code.codes/rust-runtime
- **Code:** `crates.io/crates/claude-rust-services` (or the linked GitHub mirror)
- **Shape:** `api` (Anthropic client) / `runtime` (16-module engine) / `tools` (19 tool specs) / `commands` (slash command parser) / `compat-harness` (TS interop) / `rusty-claude-cli` (terminal binary)
- **Stats:** ~20K LOC Rust for ~510K LOC of the TS Claude Code it's reimplementing. A 25× reduction.

**Why this matters for pi-crew's manifest:**

The pi-crew manifest's "Structural model" section (per `README.md` §"Structural model") describes a multi-crate workspace with `core-bridge-api` / `core-bridge-node` / `core-bridge-mock` / `core-bridge-codegen`. Claw Code proves this shape works for an *agent* system, not just for an engine-FFI bridge. Specifically:

| Claw Code crate | pi-crew analog | Notes |
|-----------------|----------------|-------|
| `api` | (no Rust analog in v1 — LLM is TS-side) | If/when Tier 2 ships, this is the shape: dedicated provider crate with typed `AuthSource`, `MessageRequest`, `StreamEvent`, `ApiError`. **Read the `api` crate description carefully** — the `AuthSource` enum has four typed variants (`None` / `ApiKey(String)` / `BearerToken(String)` / `ApiKeyAndBearer { api_key, bearer_token }`) which is exactly the kind of typed-auth the rewrite should mirror. |
| `runtime` | `engine-rs/crates/bridge/core-bridge-api/` | 16 internal modules = roughly the right size. Per the description, "core engine — bash execution, config, conversation loop, file ops, MCP, permissions, sessions." The `permissions` and `sessions` modules are direct analogs to the manifest's `WorkerPolicy` and `SessionHandle`. |
| `tools` | `engine-rs/crates/bridge/core-bridge-api/src/tool.rs` | 19 tool specs with JSON schemas + unified `execute_tool` dispatch. The dispatch function being a single verb is the pattern. |
| `commands` | (out of scope — TS-side in pi-crew) | |
| `compat-harness` | `ts/packages/native-bridge` | TypeScript/native compatibility layer. This is the package that bridges the Rust core to the existing TS front-end. Read this to understand how the boundary gets exercised. |
| `rusty-claude-cli` | (out of scope — TS-side in pi-crew) | |

**Key things to lift from Claw Code:**

1. **The `AnthropicClient` struct's field set:** `http: reqwest::Client`, `auth: AuthSource`, `base_url: String`, `max_retries: u32`, `initial_backoff: Duration`, `max_backoff: Duration`. This is the canonical "configurable client" shape. If/when the Rust core ever makes LLM calls, the `WorkerModelConfig` in the manifest should encode these fields exactly.

2. **The `ApiError` enum:**
   ```rust
   enum ApiError {
       MissingApiKey,           // typed
       ExpiredOAuthToken,       // typed
       Auth,                    // generic auth
       Http,                    // transport-level (reqwest error)
       Io,                      // filesystem / I/O
       Json,                    // serde failure
       Api { status, error_type, message, retryable },  // structured API error
       RetriesExhausted,        // terminal
   }
   ```
   This is the **right** shape for the manifest's `CoreBridgeError`. Compare to `pi_agent_rust/src/error.rs` (the cautionary example in `pi-agent-rust-port-inspiration.md`) which has `Config(String)`, `Session(String)`, etc. — all flat `String` payloads with no machine-readable discriminator. Claw Code's `ApiError` is the alternative: typed variants where the variant itself is the discriminator, plus a structured `Api { ... }` variant for "the server told us something specific" cases. **The implementing agent should adopt this shape for `CoreBridgeError`.**

3. **The 7-retryable-status-codes list:** `408, 409, 429, 500, 502, 503, 504`. Worth lifting verbatim for the `ModelStreamRetriesExhausted` error variant — matches pi-crew's existing `model-stream-retry.ts` constants.

4. **The `tools` crate's "unified `execute_tool` dispatch"** — a single function that all 19 tools route through. The manifest's `register_tool` + `ToolExecutorDescriptor` is the same idea, just expressed via FFI. The pattern of "one dispatch function, N tool impls behind it" is what makes the type system work.

5. **The `compat-harness` crate exists.** That's the proof you can have a Rust crate that *only* bridges to a TS codebase, with no business logic. The manifest's `core-bridge-node` crate is this.

**What to ignore:**
- The fact that Claw Code is a *full CLI product* with REPL, spinner, syntax highlighting. pi-crew's TUI/CLI is TS-side.
- Their slash-command parser — pi-crew's CLI commands are TS-side too.

### `schemars` — JSON Schema from Rust types

- **Crate:** `schemars` (re-exported by `rig-core` as `pub use schemars;`)
- **What it does:** `#[derive(JsonSchema)]` on a struct/enum → `T::json_schema()` returns a `serde_json::Value` of the JSON Schema.
- **Why it matters for pi-crew:**

The existing TS pi-crew has a lot of hand-written JSON Schema literals in tool definitions. If/when tool registration moves to Rust (per the manifest's `register_tool` operation with a `ToolExecutorDescriptor`), the schemas can be derived from the Rust types — no hand-translation, no drift, no schema-vs-impl mismatch. The `schemars` convention of "use `///` doc comments as field descriptions" also means the docs *are* the schema descriptions — single source of truth.

**When to use:** the moment any Rust-side tool surface exists. The pi-crew manifest's open question #2 (line 484: "Tool executor descriptors... should we ship with just `ts_builtin` and `delegated_spawn`?") becomes a no-brainer once `schemars` is in the loop — the Rust-derived schema for the dispatch enum is automatically available.

**Note:** schemars v1.0 (current) uses `#[doc]` comments for descriptions. If you see older code using `#[schemars(description = "...")]`, that's the v0.x syntax.

---

## Tier 2 — Future-relevant, defer

### `genai` (`jeremychone/rust-genai`)

- **Crate:** `genai` (v0.7.0-beta, 333K downloads/month, MIT/Apache-2.0)
- **Provider coverage:** OpenAI, Anthropic, Gemini, Ollama, AWS Bedrock (two variants), Vertex, Groq, DeepSeek, GitHub Copilot, xAI, Together, Fireworks, Cohere, Nebius, Moonshot, Aliyun, Baidu, Z.AI, BigModel, Aihubmix, OpenRouter — **27 providers, more than Rig.**
- **Distinguishing feature:** "native-protocol" — speaks each provider's wire protocol directly instead of going through OpenAI's API shape. This means Anthropic uses the Messages API, OpenAI uses Chat Completions (and has a separate `openai_resp` for Responses API), etc. — no lowest-common-denominator abstraction loss.
- **Why it's interesting for pi-crew:** if/when Rust-side LLM ever happens, genai's broader provider coverage and protocol fidelity might beat Rig's abstraction-heavy approach. Worth keeping in mind.
- **Why it's not the primary pick:** Rig's `Tool` trait + schemars integration is cleaner for our use case (we're not actually doing inference in Rust — we're dispatching typed tool calls). Genai has no equivalent `Tool` trait story.
- **Status:** pre-1.0, but with massive download velocity — clearly the most-used Rust LLM client by raw usage.

### `rust-mcp-sdk` (rust-mcp-stack)

- **Crate:** `rust-mcp-sdk` (v0.9.0, ~159K all-time downloads, maintained by Ali Hashemi)
- **Companion crate:** `rust-mcp-schema` for type-safe MCP protocol types
- **What it does:** Async MCP server + client SDK with multiple transport layers (stdio, HTTP, custom).
- **Why it matters:** pi-crew's `pi-mcp/` is currently TS-side. If MCP server implementation ever moves to Rust (e.g. for a Rust-native tool surface), this is the consensus choice. The SDK targets the latest MCP spec (2025-06-18).
- **Why defer:** MCP stays TS-side per the manifest's "what this manifest does NOT cover" section. No Rust MCP work in scope for v1.

### `rstructor` (Rust Instructor)

- **Crate:** `rstructor` (v0.4.0)
- **What it does:** `#[derive(Instructor)]` on a struct/enum generates JSON Schema, prompts the model, parses the reply, and **retries on validation errors until the data fits**. Pydantic + Instructor for Rust.
- **When useful:** if the Rust core ever needs to extract structured data from an LLM response (e.g. for a typed `CompletionPacket` payload, or for a self-orchestrating supervisor), this handles the "model returns malformed JSON, retry with schema in the prompt" loop automatically.
- **Provider coverage:** OpenAI, Anthropic Claude, Google Gemini, xAI Grok.
- **Why defer:** same LLM-in-Rust caveat. If/when relevant, it's the cleanest "structured output with auto-retry" story in the ecosystem.

### `anthropic-ai-sdk` and `bosun-ai/async-anthropic`

Both are usable, but **the more important takeaway is the pattern in Claw Code's `api` crate** (which is built on `reqwest` directly, not on either of these SDKs). If we ever bring LLM into Rust, the pattern matters more than the specific crate.

- `anthropic-ai-sdk` (lib.rs, 0.2.0, Mar 2025) — typed messages API client, streaming, files API. Less active.
- `bosun-ai/async-anthropic` — async, streaming, builder API. 73 commits, says it was forked from the unmaintained `anthropic-sdk`. Less active.

Neither has the community traction of `genai` (which covers Anthropic natively). Pick `genai` if you need Anthropic in Rust.

### `64bit/async-openai`

- **Crate:** `async-openai` (very active, the canonical OpenAI Rust client)
- **Notable feature:** **BYOT (bring-your-own-types)** — use `serde_json::Value` as request/response so you can call any OpenAI-compatible provider with the same client. Useful if pi-crew ever wants to support custom-base-URL OpenAI-compatible endpoints (e.g. local Ollama, vLLM, etc.) without per-provider code.
- **Why defer:** same LLM-in-Rust caveat. Worth noting that BYOT is the right abstraction shape for "one client, many providers" — same as genai's per-provider approach but cheaper (one impl, JSON pass-through).

---

## Tier 3 — Skip

These came up in search but are not worth the implementing agent's time:

- **`echo-agent`** — feature-bloated "LangGraph / CrewAI / AutoGen parity in Rust" framework with `#[tool]` / `#[callback]` / `#[guard]` / `#[handler]` macros and a 33-example zoo. Too opinionated, too much surface, the README pitch is feature parity not architectural soundness.
- **`mini-agent`** — 0.1.0 (March 2026), explicitly "minimal." Good educational reference for "how do you write an agent loop in Rust" but not mature enough to depend on.
- **`agent-runtime`** — "production-ready" claims, but ~0 visibility. Skip until it has more signal.
- **`agent_sdk`** — interesting cookbook recipes but the public surface is in flux.
- **`openclaw-providers`** — community fork of OpenClaw, 0.1.0. Niche.
- **`siumai-protocol-anthropic`** — looks single-provider-focused; not a unified client.
- **`claw-code-rust-services`** (the actual crate) — same content as the docs page above. Read the docs, don't pull the crate.

---

## Concrete recommendations for the rewrite

### For v1 (LLM stays TS-side, per the manifest)

1. **Don't add any LLM-side Rust dependency.** The TS side already has working code (`@earendil-works/pi-ai`). Adding Rust LLM clients now is dead weight.
2. **Borrow the *shape* of Rig's `Tool` trait** when implementing `ToolExecutorDescriptor` in `core-bridge-api`. The associated-types pattern (`NAME: &'static str`, `Args`, `Output`, `Error`) encodes dispatch-table-by-type in the type system, which is what the manifest's "operations are statically dispatched" rule wants. (Reference doc: `pi-crew-core-bridge-manifest.md` §"Tool executor descriptors" open question #2.)
3. **Borrow the *shape* of Claw Code's `ApiError` enum** for `CoreBridgeError`. Typed variants as the discriminator, structured `Api { status, error_type, message, retryable }` variant for server-side errors, terminal `RetriesExhausted` for the cap-hit case. This is the right shape for the manifest's "machine-readable + human-readable" two-channel design. (Reference doc: `pi-crew-core-bridge-manifest.md` §"Error channel".)
4. **Adopt `schemars`** the moment any Rust-side tool surface exists. The `#[derive(JsonSchema)]` + `///` doc-comment convention means schemas and docs share a source.
5. **Read the Claw Code Rust runtime docs page** (https://claw-code.codes/rust-runtime) before writing the workspace `Cargo.toml`. The 6-crate split is proven for an agent system; we don't need to invent it.

### For future Tier 2 (if/when LLM moves into Rust)

1. **Pick Rig over genai if** you want a clean `Tool` trait story and don't mind 7K-star-but-pre-1.0 churn risk. **Pick genai if** you want maximum provider coverage (27+) and protocol fidelity, and you're willing to write your own tool-dispatch layer.
2. **Don't pick `anthropic-ai-sdk` or `bosun-ai/async-anthropic` directly.** Both genai and Rig handle Anthropic as a first-class provider; pulling in a single-provider SDK leaves you with the multi-provider problem unsolved.
3. **Pin versions aggressively.** All of these are pre-1.0 and the maintainers say so. Treat version bumps as architectural decisions, not routine upgrades.

---

## Sources

- Rig: https://github.com/0xPlaygrounds/rig · https://rig.rs · https://docs.rs/rig-core
- Rig Tool docs: https://docs.rig.rs/docs/concepts/tools
- genai: https://github.com/jeremychone/rust-genai
- Claw Code Rust runtime: https://claw-code.codes/rust-runtime
- Claw Code crate: https://crates.io/crates/claude-rust-services
- schemars: https://docs.rs/schemars (re-exported by Rig)
- rust-mcp-sdk: https://crates.io/crates/rust-mcp-sdk
- rstructor: https://crates.io/crates/rstructor · https://lib.rs/crates/rstructor
- instructor-rs: https://github.com/instructor-ai/instructor-rs
- async-openai: https://github.com/64bit/async-openai
- openai_dive: https://crates.io/crates/openai_dive
- anthropic-ai-sdk: https://lib.rs/crates/anthropic-ai-sdk
- bosun-ai/async-anthropic: https://github.com/bosun-ai/async-anthropic
- mini-agent: https://lib.rs/crates/mini-agent · https://github.com/RajMandaliya/mini-agent
- echo-agent: https://docs.rs/echo_agent/latest/echo_agent
- agent_sdk: https://docs.rs/agent-sdk
- agent-runtime: https://crates.io/crates/agent-runtime
- openclaw-providers: https://lib.rs/crates/openclaw-providers
- Rust LLM ecosystem overview: https://lib.rs/ai
- 5-year retrospective on Claude Code's Rust rewrite: https://dev.to/brooks_wilson_36fbefbbae4/claude-code-architecture-explained-agent-loop-tool-system-and-permission-model-rust-rewrite-41b2
