# Rusty Crew

Rusty Crew is a service runtime for persistent AI agents. One service can host
multiple full agents, delegated workers, and external Codex app-server
sessions while keeping coordination, lifecycle, and durable service data under
one Rust-owned authority.

The project began as a structural successor to pi-crew, but it is now a working
service rather than a scaffold. It provides profile and provider management,
two production LLM brain loops, model-callable local and MCP tools, durable
sessions and chat streams, direct agent messaging, delegation and worker
lifecycle, roleplay storage/runtime surfaces, admin diagnostics, and SQLite and
PostgreSQL storage backends.

Rusty Crew is still under active development. Current local deployments contain
real agents, but API and storage contracts may still make deliberate breaking
changes when that produces a cleaner long-term path.

## Start Here

- [Rusty View](https://github.com/FuzzySlipper/rusty-view) is the companion web
  client for chatting with Crew brain and managed Codex sessions and operating
  a Rusty Crew service from the browser.
- [Deployment and storage](docs/deployment-and-storage.md) explains service
  roots, systemd setup, SQLite and PostgreSQL configuration, health checks,
  backups, and the live/debug split.
- [Model providers](docs/model-providers.md) explains provider aliases,
  supported protocols and credentials, provider APIs, OAuth, profile
  assignment, and runtime refresh.
- [Docs index](docs/README.md) maps the architecture records, ADRs, contracts,
  live-test procedures, and historical material.
- [Local service runbook](docs/local-service-runbook.md) is the detailed
  operator reference for the current source-run deployment.
- [API capability registry](docs/api-capability-command-catalog-ratchet.md) documents the
  queryable API and slash-command registry.
- [Runtime activity census](docs/runtime-activity-census.md) documents the
  Rust-owned active-work ledger, topology, mismatch findings, and privacy
  boundary used to diagnose detached or apparently idle agent work.
- [Review-agent inbox guidance](docs/review-agent-inbox-and-prompt-guidance.md)
  provides copy-ready prompts and the durable serial review contract.
- [External agent review CLI](docs/external-agent-review-cli.md) documents the
  authenticated exact-SHA review path for unmanaged agents, including explicit
  production/debug selection and polling.
- [Built-in Rusty Crew help skill](docs/skills-list-view-tools.md) documents the
  immutable help surface available to every native Crew brain, its compact
  prompt pointer, and its queryable diagnostics.

## Current Architecture

Rust owns deterministic coordination and production brain execution:

- the in-process agent bus, sessions, wakes, queues, TTL, and restart hydration;
- brain selection and the production `chat-completions` and `openai-responses` loops;
- action validation, delegation and worker lifecycle, completion routing, and
  runtime counters;
- backend-neutral persistence for Crew-owned data, including profiles,
  providers, transcripts, memory, lore, and module records.

TypeScript is the service composition and capability boundary:

- HTTP/admin/chat route composition and the Node native-addon host;
- model-callable tool implementations and Rust-issued tool execution;
- profile/role assembly, MCP clients, skills, and platform adapters;
- Den, Telegram, TUI, browser, and external-runtime integration.

Brain implementations use a neutral wake/stream/action/provider-state
contract. Provider protocol selects the built-in production brain unless a
compatible module is explicitly configured:

| Provider protocol  | Brain              | Intended provider surface                                       |
| ------------------ | ------------------ | --------------------------------------------------------------- |
| `chat_completions` | `chat-completions` | OpenAI-compatible chat-completions APIs                         |
| `responses`        | `openai-responses` | OpenAI Responses-compatible APIs, including direct OpenAI OAuth |

External Codex app-server sessions are managed through a separate external
runtime boundary. They preserve the official Codex loop and tool behavior while
participating in Crew chat, metadata, lifecycle, and coordination surfaces.

## Service Model

A Rusty Crew deployment is one service process managing a fleet of agents. It
is not one process per profile. Profiles reference database-backed provider
aliases and local tool profiles; MCP servers are explicit per-profile bindings
rather than an implied Den connection.

Crew owns its service data. Den remains the source of truth for Den projects,
tasks, documents, and observability, but it is not Crew's internal bus or an
alternate persistence backend.

The current development machine intentionally runs two isolated deployments:

| Purpose            | Root                            | Port   | Storage    |
| ------------------ | ------------------------------- | ------ | ---------- |
| Long-lived agents  | `/home/system/rusty-crew`       | `9347` | PostgreSQL |
| Disposable testing | `/home/system/rusty-crew-debug` | `9348` | SQLite     |

Live tests, temporary profiles, and frontend certification belong on the debug
service. Never point two service processes at the same runtime root, SQLite
file, or PostgreSQL schema.

## Repository Map

```text
crates/
  core/                 Rust coordination, config, tools, and persistence
  brains/               Rust chat-completions and OpenAI Responses brain loops
  bridge/               napi bridge API, Node addon, and contract codegen
  roleplay/             Rust-owned roleplay domain/storage authority
ts/packages/
  service-host/         Process composition and HTTP listener
  brain-island/         Tools, profile/role assembly, routes, and host adapters
  native-bridge/        Generated/typed Node boundary
  contracts/            Generated and transitional TypeScript contracts
  adapter-*/            Den, MCP, Telegram, TUI, and other external adapters
governance/             Dependency and storage ownership rules
ops/                    systemd units, backup timers, and operator scripts
docs/                   ADRs, contracts, runbooks, proof records, and history
```

`governance/ownership.toml` and `governance/storage-scope.toml` are
machine-checked boundaries. Extend those maps when adding a crate or durable
storage scope instead of relying only on prose conventions.

## Build And Verify

The pinned toolchains are Rust `1.96.0`, Node `v26.2.0`, and npm `11.16.x`.
From a fresh checkout:

```bash
npm ci
npm run build:native
npm run verify:offline
```

`verify:offline` runs Rust format, clippy, and workspace tests; TypeScript
typechecking and unit tests; architecture and runtime-config checks; native
bridge contract/codegen drift checks; and the deterministic offline smoke lane.
It does not require Den, a running service, PostgreSQL, Rusty View, or a live
model provider.

List and select broader integration smokes with:

```bash
npm run smoke -- --list
npm run smoke -- --list --lane debug-service
npm run smoke -- brain
```

Substantial chat/runtime work is not considered field-certified by synthetic
tests alone. Use the debug service plus a live provider and Rusty View according
to [live deliverable certification](docs/live-deliverable-certification.md).

## Run A Service

The service currently runs from the source checkout and stores mutable state in
a separate runtime root. After creating a service environment as described in
the [deployment guide](docs/deployment-and-storage.md):

```bash
npm run service:preflight
npm run service:start
```

The repo also includes user-systemd units:

```bash
cp ops/systemd/rusty-crew.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now rusty-crew.service
```

Shallow health is unauthenticated; readiness and diagnostics use the configured
admin auth mode:

```bash
curl http://127.0.0.1:9347/v1/admin/healthz
curl -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/readyz
curl -H "Authorization: Bearer $RUSTY_CREW_ADMIN_TOKEN" \
  http://127.0.0.1:9347/v1/admin/diagnostics/activities
```

When a static frontend is copied to `<runtime-root>/site`, Crew serves it at
`/` alongside `/v1/*` APIs and the built-in `/admin` diagnostics page.

## Source Of Truth

- Den project `rusty-crew` owns current planning, task state, and live guidance.
- `rusty-crew-unified-architecture` is the authoritative architecture document;
  `brain-body-architecture` is its primary companion.
- Repo ADRs and docs describe landed implementation and operator contracts.
- Code and tests are implementation truth when older audits or planning notes
  disagree with landed behavior.
- `docs/historical/` preserves superseded analysis as history, not as current
  setup guidance.

## Core Rules

- Rust owns coordination, lifecycle, production brain loops, and Crew storage.
- TypeScript stays at explicit composition, tool, and external-adapter
  boundaries; do not route around Rust authority.
- Tool availability is profile-based. Do not restore `WorkerPolicy` as the main
  tool gate.
- Every native Crew brain receives the immutable `rusty_crew_help` tool. The
  full help body is loaded on demand and cannot be shadowed by filesystem
  skills or removed by profile policy.
- MCP servers are explicit profile bindings and may include any number of
  independent servers.
- Queued instructions and messages require explicit, aggressive TTL. Expired
  work must not be resurrected by restart or reconnect.
- Intentional stubs, fakes, and partial paths require a discoverable follow-up
  task and known-limitation record.
- Use current pi sources from `https://github.com/earendil-works/pi` only when
  pi reference behavior is needed; old local package locations are historical.
