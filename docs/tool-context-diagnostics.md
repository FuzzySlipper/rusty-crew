# Tool and Context Diagnostics

Task: Den `2961`

Rusty Crew now has a pure TypeScript report builder for explaining why a session has the tool and context surface it has. The report is intended for admin routes, the debug TUI, and slash command status/session responses.

## Scope

The report combines:

- tool registry diagnostics and inventory state
- profile tool policy and per-session constraints
- role assembly summaries
- resource limit summaries
- channel and MCP adapter diagnostics

Tools are normalized into four operator-facing states:

- `selected`: available to the brain for this session
- `denied`: present, but excluded by profile, session, resource, deprecation, alias, or not-requested policy
- `missing`: explicitly requested but not registered
- `collided`: blocked by registry validation or capability/name collisions

The detailed `reasonCodes` preserve the specific cause so an operator can tell the difference between a read-only resource denial, a profile denial, a missing MCP tool, or a registry collision.

## Context Privacy

Role assembly diagnostics intentionally avoid dumping raw prompt or skill bodies by default. They expose counts, section headings, skill metadata, body lengths, and SHA-256 hashes for the system prompt and instruction text.

This lets an admin or TUI confirm that the expected context was assembled without leaking full prompts, secrets, or transient conversation content into routine diagnostics output.

## Adapter And Resource Explanations

The report filters channel and MCP diagnostics to the current session or profile. It surfaces:

- channel binding count, statuses, and degraded notes
- MCP surface count, statuses, server names, and collision count
- workdir scope
- duration and delegation depth limits
- read-only session effects on write/process tools

Tool rows expose canonical metadata and inventory explanations. They do not
include executor module pointers by default; those remain an intentional
tool-registry debug binding view rather than part of the public diagnostics
contract.

## Verification

`npm run smoke:tool-context-diagnostics` covers selected, denied, missing, and collided tools, degraded MCP surfaces, resource/workdir explanations, and prompt privacy behavior.
