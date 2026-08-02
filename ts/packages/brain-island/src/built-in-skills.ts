import { createHash } from "node:crypto";

export const RUSTY_CREW_BUILT_IN_SKILL_SLUG = "rusty-crew";
export const RUSTY_CREW_BUILT_IN_SKILL_VERSION = "1.0.0";
export const RUSTY_CREW_BUILT_IN_SKILL_SOURCE =
  "builtin://rusty-crew/skills/rusty-crew";

export interface BuiltInSkill {
  slug: string;
  title: string;
  summary: string;
  tags: readonly string[];
  bodyMarkdown: string;
  sourcePath: string;
  source: "built_in";
  immutable: true;
  contentVersion: string;
  contentFingerprint: string;
}

export interface BuiltInSkillCatalogDiagnostics {
  schemaVersion: 1;
  ok: true;
  catalogId: "rusty-crew-built-in-skills";
  registeredSkillCount: number;
  promptPointer: {
    available: true;
    bodyEmbedded: false;
    chars: number;
    fingerprint: string;
  };
  skills: readonly Pick<
    BuiltInSkill,
    | "slug"
    | "title"
    | "sourcePath"
    | "source"
    | "immutable"
    | "contentVersion"
    | "contentFingerprint"
  >[];
}

const rustyCrewHelpBody = `# Rusty Crew

Rusty Crew is a service-hosted agent runtime. It provides native Crew brain
sessions, profiles, tools, memory surfaces, inter-agent coordination, and a
browser-facing API used by Rusty View.

## Identify The Runtime

- A native Crew brain runs through Rusty Crew's chat-completions or Responses
  brain and receives the profile's prompt, tool policy, MCP servers, and memory
  policy.
- A managed Codex app-server session is hosted and observed by Rusty Crew, but
  Codex owns its agent loop and built-in tools. This skill describes native
  Crew behavior unless a section explicitly says otherwise.
- Rusty View is a client for Crew. It is not the agent runtime or the authority
  for session state.

## Profiles And Providers

A profile is the durable configuration for an agent identity. It selects a
provider alias, native brain implementation, local tool profile, MCP servers,
prompt assets, memory policy, and runtime defaults. Provider aliases are
service-owned records containing endpoint/model behavior and credentials.

Do not infer the active model from a profile name. Use the model/status command
or the running service's session and profile diagnostics. Configuration changes
may require a runtime refresh or a new logical session before they affect an
already-active brain.

## Sessions And Commands

Sessions hold conversation history and runtime state. Starting a new session is
an explicit operation; service restarts should not silently replace the active
session. Archiving removes a session from active navigation while preserving
history according to service policy. Cancellation stops an active turn without
destroying the session.

Use the running service's command registry for the current command list and
descriptions. Common concepts include status/model inspection, effort override,
explicit new-session creation, cancellation, and archival. Do not assume a
command exists only because another harness supports the same spelling.

## Tools, MCP, And Skills

Local tools come from a service tool registry and are grouped into local tool
profiles. MCP servers are explicit per-profile connections; Den is one possible
MCP server and is not implicitly coupled to Crew. Inspect the active tool
inventory before promising a capability.

Skills are guidance, not executable authority. This immutable built-in skill is
always available to native Crew brains through rusty_crew_help. Filesystem
skills remain profile-selected and separately governed. The built-in cannot be
shadowed, edited, archived, or replaced by a filesystem skill.

## Memory And Delegation

Crew has distinct memory surfaces: profile prompt assets, dense profile memory,
session memory, optional external memory, roleplay lore, and runtime search.
They have different ownership and retention rules. Do not describe Den tasks or
documents as memory; use the profile's Den MCP tools when those are configured.

Delegated workers and subagents are bounded child executions. Treat a delegated
completion as evidence to inspect, not as unquestionable authority. Native
agent-to-agent messages use Crew's coordination bus and TTL-aware delivery.
Switchboard aliases are operator-defined routes to stable target sessions.

## Long Turns And Failure Recovery

Crew supports long-running turns and explicit continuation. A provider, tool,
storage, or routing failure should be surfaced with a reason code and available
debug details. Report a dependency failure plainly rather than repeatedly
calling the same failing tool. If a turn appears stuck, inspect session events,
active execution diagnostics, provider snapshots, and persistence health before
restarting the service.

## Operator And Debug Surfaces

Use the service diagnostics and Rusty View inspector to distinguish profile,
session, wake, provider, tool, persistence, and external-runtime failures.
Production and debug Crew instances are separate services and databases; never
assume an operation against one affects the other.

Agent-facing tools expose only the capabilities granted to the current native
session. Operator/admin APIs can create profiles, manage providers and sessions,
refresh configuration, inspect diagnostics, and perform emergency control.
Do not claim operator privileges from an agent tool inventory.

## Current Source

- Rusty Crew: https://github.com/FuzzySlipper/rusty-crew
- Rusty View: https://github.com/FuzzySlipper/rusty-view

The running service's queryable command, capability, tool, and diagnostics
registries are more current than a remembered static list. Use them whenever
exact behavior matters.
`;

function fingerprint(content: string): string {
  return `sha256:${createHash("sha256").update(content).digest("hex")}`;
}

export const rustyCrewBuiltInSkill: BuiltInSkill = Object.freeze({
  slug: RUSTY_CREW_BUILT_IN_SKILL_SLUG,
  title: "Rusty Crew",
  summary:
    "Authoritative, provider-neutral help for the Rusty Crew runtime and its native brain sessions.",
  tags: Object.freeze(["rusty-crew", "harness", "help"]),
  bodyMarkdown: rustyCrewHelpBody,
  sourcePath: RUSTY_CREW_BUILT_IN_SKILL_SOURCE,
  source: "built_in",
  immutable: true,
  contentVersion: RUSTY_CREW_BUILT_IN_SKILL_VERSION,
  contentFingerprint: fingerprint(rustyCrewHelpBody),
});

export const RUSTY_CREW_HARNESS_PROMPT_POINTER = [
  "# Rusty Crew Harness",
  "This is a native Rusty Crew brain session, not a managed Codex app-server session.",
  "For harness help, call rusty_crew_help to read the immutable built-in rusty-crew skill.",
  "Use the running service command and capability registries when exact commands or API behavior matter.",
].join("\n");

export const RUSTY_CREW_HARNESS_PROMPT_POINTER_FINGERPRINT = fingerprint(
  RUSTY_CREW_HARNESS_PROMPT_POINTER,
);

export function listBuiltInSkills(): readonly BuiltInSkill[] {
  return [rustyCrewBuiltInSkill];
}

export function getBuiltInSkill(slug: string): BuiltInSkill | undefined {
  return slug === RUSTY_CREW_BUILT_IN_SKILL_SLUG
    ? rustyCrewBuiltInSkill
    : undefined;
}

export function isReservedBuiltInSkillSlug(slug: string): boolean {
  return slug === RUSTY_CREW_BUILT_IN_SKILL_SLUG;
}

export function builtInSkillCatalogDiagnostics(): BuiltInSkillCatalogDiagnostics {
  return {
    schemaVersion: 1,
    ok: true,
    catalogId: "rusty-crew-built-in-skills",
    registeredSkillCount: 1,
    promptPointer: {
      available: true,
      bodyEmbedded: false,
      chars: RUSTY_CREW_HARNESS_PROMPT_POINTER.length,
      fingerprint: RUSTY_CREW_HARNESS_PROMPT_POINTER_FINGERPRINT,
    },
    skills: listBuiltInSkills().map((skill) => ({
      slug: skill.slug,
      title: skill.title,
      sourcePath: skill.sourcePath,
      source: skill.source,
      immutable: skill.immutable,
      contentVersion: skill.contentVersion,
      contentFingerprint: skill.contentFingerprint,
    })),
  };
}
