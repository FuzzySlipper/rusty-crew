# Web Browser Tool Safety Architecture

Status: Design contract for task 2880

Date: 2026-06-20

## Scope

Rusty Crew needs pi-crew parity for:

- `web_search`
- `web_extract`
- `browser_navigate`
- `browser_snapshot`
- `browser_click`
- `browser_type`
- `browser_scroll`
- `browser_back`
- `browser_press`
- `browser_console`
- `browser_vision`

These are TypeScript brain-island tools. Rust owns coordination, session
identity, session/resource policy, durable events, and lifecycle authority. Rust
does not own HTTP fetching, CDP control, DOM interaction, or screenshot capture.

The implementation goal is not to make browser tools globally powerful. The goal
is to provide bounded research and interaction tools that are safe to expose to a
profile-selected brain, observable by operators, and easy to clean up.

## Ownership

### TypeScript Brain Island Owns

The brain island owns:

- web search provider calls;
- URL extraction/fetching;
- SSRF and private-network checks at fetch time;
- redirect handling and content limits;
- CDP subprocess/session management;
- page navigation and interaction;
- accessibility snapshots and browser refs;
- console capture;
- screenshot capture;
- model-facing tool result shaping.

The brain island must fail closed if a guardrail cannot be checked.

### Rust Coordination Owns

Rust owns:

- session identity and profile identity;
- selected `ToolProfile` and resource limits;
- wake/session lifecycle;
- durable tool-call telemetry;
- resource hook decisions that are independent of HTTP internals;
- cancellation/deadline policy passed to TS;
- restart/shutdown lifecycle signals.

Rust may record that `web_extract` was attempted, denied, completed, or failed.
Rust should not attempt to validate every redirect, parse DOM snapshots, or own
CDP state.

### Adapters Own

Adapters own projection to external systems. Web/browser tools should not post
directly to Den Channels, Telegram, MCP, or an admin UI. They return structured
tool results and telemetry. Projection layers decide what is operator-visible.

## Tool Registry Contract

Web/browser tools must be canonical registry entries before they are exposed to a
brain.

Suggested registry metadata:

| Tool | Category | Toolsets | Safety |
| ---- | -------- | -------- | ------ |
| `web_search` | `web` | `web_research` | `read_only`, `network_access` |
| `web_extract` | `web` | `web_research` | `read_only`, `network_access` |
| `browser_navigate` | `browser` | `browser` | `network_access` |
| `browser_snapshot` | `browser` | `browser` | `read_only` |
| `browser_click` | `browser` | `browser` | `external_write` |
| `browser_type` | `browser` | `browser` | `external_write` |
| `browser_scroll` | `browser` | `browser` | `read_only` |
| `browser_back` | `browser` | `browser` | `read_only` |
| `browser_press` | `browser` | `browser` | `external_write` |
| `browser_console` | `browser` | `browser` | `read_only` |
| `browser_vision` | `browser` | `browser_vision` | `read_only` |

`browser_vision` should not make a model/provider call in v1. V1 may produce a
screenshot artifact/ref. Vision analysis should later route through the normal
provider/brain path with profile policy and result refs.

## Web Search

`web_search` is safe to port as a provider abstraction from pi-crew if the
Rusty Crew version keeps provider selection out of model input.

Provider policy should be runtime/profile configuration:

- default provider;
- allowed providers;
- request timeout;
- max results;
- safe-search/options where supported;
- whether network access is allowed for the session.

Tool input may include query and bounded result count. It must not include
provider credentials, arbitrary endpoint URLs, proxy configuration, or private
network escape flags.

## Web Extract Network Guardrails

`web_extract` must preserve pi-crew's SSRF posture and be stricter where runtime
shape allows it.

Default policy:

- allow only `http:` and `https:`;
- reject embedded credentials in URLs;
- reject non-standard ports unless policy explicitly allows them;
- resolve hostnames before fetch;
- block loopback, link-local, private, carrier-grade NAT, multicast,
  unspecified, reserved, and metadata-service addresses;
- treat IPv4-mapped IPv6 as IPv4 and re-run private-network checks;
- cap redirects, default 5;
- re-run full URL and network checks on every redirect;
- cap response bytes;
- cap extracted text chars;
- cap content type to HTML/text/JSON-like safe text unless explicitly allowed;
- cap total request duration;
- avoid sending ambient credentials/cookies;
- use a stable Rusty Crew user agent;
- fail closed on DNS/URL parsing ambiguity.

Private-network access must be a runtime configuration escape hatch, not a tool
argument. Prefer `RUSTY_CREW_ALLOW_PRIVATE_NET=1` or equivalent service config.
Keep support for pi-crew's old `PI_CREW_ALLOW_PRIVATE_NET=1` only as a temporary
compatibility alias if migration needs it; do not document it as the Rusty Crew
primary switch.

Guardrails must account for DNS rebinding. The implementation should either use
an HTTP client/agent that can pin and inspect the resolved address for the
connection, or it must fail closed for hostnames where the resolved address
cannot be verified at connect time.

Redirect handling must validate both the redirect target URL and the resolved
address before following. A public first URL redirecting to localhost/private
network is denied.

## Browser Session Scope

Default browser state scope: per Rust `SessionId`.

Rationale:

- per wake is safest but loses continuity for multi-step browser work;
- profile-scoped state risks cross-agent and cross-task leakage;
- per Rust session gives continuity without sharing state across agents;
- session archive/shutdown gives a clear cleanup signal;
- diagnostics and tool telemetry can attribute browser state to one agent
  session.

The browser manager should be keyed by `SessionId`. If an agent has multiple
parallel sessions, each gets a separate browser session. Delegated sessions do
not inherit the parent's browser by default.

Optional future policy may allow explicit parent/child browser sharing, but only
through a resource policy and visible diagnostics.

## Browser Process Lifecycle

The TS browser manager should own Chromium/CDP subprocesses and expose a narrow
session manager API to tools.

Required behavior:

- lazy start on first browser tool call for a session;
- one browser context per Rust `SessionId`;
- no profile-global cookies/storage by default;
- ephemeral user data directories;
- configured max concurrent browser sessions;
- idle timeout cleanup;
- hard lifetime cap;
- cancellation via `AbortSignal` or equivalent timeout control;
- cleanup on session archive/shutdown signal when available;
- orphan cleanup on startup for known temp dirs/process records;
- bounded console log ring buffer;
- bounded snapshot/ref cache;
- no raw browser refs shared across sessions;
- no unbounded screenshots in memory.

Restart behavior:

- do not restore browser sessions after process restart;
- old refs become invalid;
- diagnostics may report that a browser session was dropped;
- tools should return a clear stale-ref/session-gone error.

## Browser Navigation Guardrails

Browser navigation must reuse the web network policy where possible.

V1 browser navigation should block:

- `file:`, `data:`, `javascript:`, `chrome:`, `devtools:`, and extension URLs;
- loopback/private/metadata hosts unless runtime policy allows private network;
- credentialed URLs;
- downloads unless explicit download support is later designed;
- popups/new windows unless explicitly opened by the session manager.

Navigation redirects and subresource requests are harder to police than
`web_extract`. The browser manager should at minimum validate top-level
navigation targets and block top-level redirects to denied schemes/hosts. Later
resource interception can enforce subresource policy if needed.

## Browser Refs And Snapshots

`browser_snapshot` should produce accessibility snapshots and stable-ish element
refs scoped to the browser session and snapshot generation.

Rules:

- refs include session id and snapshot generation internally;
- tools accept refs, not arbitrary selectors, for click/type/press where
  possible;
- stale refs fail closed with a snapshot refresh hint;
- snapshot output is bounded;
- hidden/private form values should be redacted where possible;
- no full DOM dump by default;
- console output is bounded and redacted for obvious secret patterns.

The pi-crew snapshot/ref pattern is safe to adapt, but the Rusty Crew version
must add explicit session scoping because the service is expected to host many
agents.

## Browser Vision

V1 `browser_vision` should be a screenshot capture and result-ref tool.

It may:

- capture a viewport screenshot;
- return dimensions, media type, and artifact ref;
- emit tool telemetry;
- optionally provide a small textual note such as current URL/title.

It must not:

- call a vision model directly;
- dump a large base64 screenshot into normal channel text;
- store screenshots in unbounded memory;
- expose screenshots across sessions.

Future model-backed analysis should use the normal model/provider path and
reference the screenshot artifact.

## Copy/Adapt Guidance From Pi-Crew

Safe to copy/adapt closely:

- `web_search` provider abstraction and result shaping;
- `web_extract` URL parsing, redirect cap, private-network detection, IPv4-mapped
  IPv6 checks, response extraction limits;
- browser tool names and basic model-facing parameter shapes;
- accessibility snapshot/ref approach;
- console ring-buffer idea.

Must be redesigned or constrained:

- any global browser singleton;
- browser state that is profile-scoped by default;
- any private-network escape hatch exposed as a model argument;
- any direct browser vision model call;
- any tool result that returns large screenshots or full DOM dumps inline;
- any tool implementation that bypasses Rusty Crew's canonical registry,
  selected tool profile, or telemetry hooks.

## Telemetry And Diagnostics

Every web/browser tool call should emit or record:

- tool name;
- session id/profile id;
- URL host or provider name where safe;
- allow/deny decision;
- reason code for denials;
- duration;
- byte/snapshot/screenshot size where applicable;
- redirect count for `web_extract`;
- browser session id/ref generation for browser tools;
- safe error category.

Do not record raw page content, full screenshots, cookies, auth headers, or full
DOM snapshots in durable telemetry.

Diagnostic surfaces should report:

- active browser sessions by owning Rust session;
- idle age and lifetime age;
- current URL/title if safe;
- snapshot/ref counts;
- console ring size;
- last error/reason code;
- cleanup counts;
- whether private network access is enabled.

## Implementation Order

1. Register web/browser tool metadata in the canonical registry, initially
   disabled or profile-gated.
2. Port `web_search` provider abstraction.
3. Port `web_extract` with network guardrails and tests before exposing it to
   profiles.
4. Add browser session lifecycle design/API and diagnostics.
5. Implement CDP browser session manager.
6. Add `browser_navigate` and `browser_snapshot`.
7. Add ref-based interactions: click, type, scroll, back, press.
8. Add console capture.
9. Add screenshot capture/result refs for `browser_vision`.
10. Prove web and browser tools through real Rust-driven brain wakes.

## Open Follow-Ups

- Decide whether downloads are permanently denied or separately supported.
- Decide artifact storage for screenshots and large extracted content.
- Decide how Rust session resource limits map into TS network/browser deadlines.
- Decide whether subresource interception is required before browser navigation
  can be enabled for broader profiles.
