# Platform Adapter Anti-Corruption Audit

Status: implementation plan for Den task 4588
Date: 2026-07-07

## Purpose

Rusty Crew's platform adapters should translate between external systems and
Crew contracts. They should not become secondary coordination buses, hidden
runtime policy engines, or independent lifecycle authorities.

The current adapter packages are mostly on the right side of that boundary:
they own HTTP/SSE/client calls, auth headers, retries, external payload
normalization, and display-specific rendering. The audit found several places
where route, lifecycle, and tool metadata policy should either move to Rust or
be explicitly validated by Rust-owned planners.

## Current Adapter Surfaces

- `adapter-den`: Den successor gateway clients, channel ingress/projection,
  product-data ingress, Den memory client, router metadata, and channel route
  resolution.
- `adapter-telegram`: Telegram update polling, update normalization, outbound
  request shaping, binding lookup, TTL/idempotency, and diagnostics counters.
- `adapter-mcp`: MCP discovery, tool candidate conversion, schema
  normalization, model-callable wrapper creation, and execution result mapping.
- `adapter-tui`: debug TUI rendering over admin/debug API clients.

## Healthy TypeScript Glue

The following responsibilities should remain in TypeScript adapters:

- external HTTP/SSE/client SDK calls;
- transport retries and cursor persistence;
- auth/header handling;
- external payload normalization into Crew contract shapes;
- outbound payload formatting for external systems;
- bounded diagnostics counters for adapter health;
- display-only TUI rendering;
- model-callable wrappers around external MCP tool execution.

These are anti-corruption-layer jobs. They translate external systems into
Crew-shaped requests and projections without owning Crew lifecycle truth.

## Authority Risks

### Channel Routing

`adapter-den/src/channel-routing.ts` currently chooses a target binding using
explicit binding id, mentions, runtime agent hints, and single-binding fallback.
`adapter-telegram/src/index.ts` has its own binding match and ambiguity logic.

That logic is practical adapter glue, but the final decision to wake or route a
message to a Crew agent should be Rust-owned or Rust-validated:

- binding status and profile/session target validity;
- ambiguity/denial reason codes;
- TTL/idempotency acceptance;
- whether a channel message should wake an agent;
- correlation/idempotency keys used by the internal bus.

Adapters can still normalize provider refs and collect mention hints. Rust
should decide whether those hints resolve to a route.

### Den Product Ingress

`den-product-ingress.ts` correctly denies non-observe lifecycle operations, but
the denial is TS-local. Product updates from Den should remain reference data;
claim/complete/retry/expire operations must not become Crew lifecycle commands
through the adapter.

The denial policy should be Rust-validated or represented in a shared
contracted policy so future adapter changes cannot accidentally turn Den product
events into runtime mutations.

### MCP Discovery And Tool Metadata

`adapter-mcp/src/mcp-discovery.ts` normalizes MCP tool names, schemas,
toolsets, safety flags, output shapes, and optional argument cleanup. That is a
large amount of model-facing policy for an adapter.

External MCP clients and execution wrappers can stay in TS, but Rust/tool
registry policy should validate or generate:

- model-callable tool names;
- source identity and collision handling;
- category/toolset/surface metadata;
- safety flags and external-write hints;
- schema normalization diagnostics;
- output-shape and inventory-test expectations.

### TUI

`adapter-tui` is display-only. It should remain TS presentation code. It should
not inspect storage, mutate runtime state, or invent diagnostics unavailable
from public admin/debug APIs.

## Migration Slices

1. Define a Rust channel ingress route planner. Adapters pass normalized
   provider refs, message metadata, mention hints, binding candidates, and TTL
   information; Rust returns route/deny/ambiguous/expired/duplicate decisions
   with stable reason codes.
2. Move Telegram binding resolution to the same channel route planner used by
   Den channel ingress, leaving Telegram-specific update normalization in TS.
3. Add a Den product ingress policy guard in Rust/shared contracts so only
   observation/reference injection is accepted through adapters. Lifecycle
   operations must require explicit Crew control-plane commands.
4. Move MCP discovered-tool metadata validation to `core-tool-registry` or a
   generated bridge contract. TS discovery can produce candidates, but Rust must
   validate model-facing metadata before registration.
5. Add adapter boundary ratchets: no adapter package may call internal routing,
   wake, lifecycle, or persistence mutation APIs except through approved
   bridge/control-plan functions.
6. Certify with deterministic adapter smokes and live debug-service checks that
   channel routing, MCP discovery, and Den product ingress report explicit
   decisions without adapters becoming coordination authority.

## Non-Goals

- Do not move external HTTP clients or SDK wrappers into Rust just for purity.
- Do not make Den channels or Telegram a second internal bus.
- Do not let Den product data claim/complete task lifecycle on Crew's behalf.
- Do not hand-maintain duplicate MCP tool policy in adapter code after Rust
  validation exists.
- Do not require the TUI to know private runtime internals.

## Acceptance For The Series

- Platform adapters are documented as glue/presentation versus Rust authority.
- Channel route/wake/lifecycle decisions are made or validated by Rust-owned
  operations with stable reason codes.
- MCP model-tool metadata from external discovery cannot register solely through
  adapter-side policy.
- Den and Telegram adapters use the same route-decision contract after
  provider-specific normalization.
