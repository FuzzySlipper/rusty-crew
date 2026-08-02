# Skills List And View Tools

Task: Den `2902`

Rusty Crew has one immutable built-in help tool and two read-only skill
inspection tools:

- `rusty_crew_help`
- `skills_list`
- `skill_view`

`rusty_crew_help` reads the compiled-in `rusty-crew` skill. It is selected for
every native Crew brain independently of the profile's local tool profile,
filesystem skill roots, and `skills_read` selection. Managed Codex app-server
sessions keep their own harness behavior and are not modified by this tool.

The built-in catalog is deliberately small and immutable. Its reserved
`rusty-crew` slug cannot be shadowed, edited, archived, curated, or injected as
a profile-selected filesystem skill. It exposes a semantic content version and
SHA-256 fingerprint so diagnostics and tests can identify the exact help body.
No seed, database row, runtime directory, or profile edit is needed.

## Behavior

`skills_list` reads configured skill roots and returns skill metadata:

- slug
- title
- summary
- tags
- source path
- source kind (`built_in` or `filesystem`)
- immutable flag
- content version and fingerprint
- status
- parse error, when invalid metadata is included

The built-in entry is always present. By default invalid filesystem skills are
hidden from the list. Passing `includeInvalid` includes them with status
`invalid` and an error message. A missing, empty, or unreadable filesystem root
is returned as a diagnostic while the built-in catalog remains usable.

`skill_view` reads one skill by safe slug. It can include or omit the body and
supports body truncation through `maxBodyChars`.

## Safety

The tools are read-only. `skill_view` accepts a slug, not an arbitrary path, and
slugs must match a safe filename pattern. If `allowedSkills` is supplied in the
tool context, view/list operations are restricted to that profile-visible set.

Missing or invalid skill roots return structured diagnostics instead of
throwing raw filesystem errors into the model turn. Mutation attempts against
the reserved slug return `built_in_skill_immutable` before any filesystem or
curator policy is consulted.

## Prompt And Diagnostics

Native profile role assembly includes a small provider-neutral pointer after
profile-owned soul, memory, and instruction content. The pointer identifies the
native Crew harness and tells the model to call `rusty_crew_help`; it does not
embed the full skill body in every provider request. Explicit system-prompt
overrides do not remove this pointer.

`GET /v1/admin/diagnostics/built-in-skills` reports registration and prompt
pointer health directly. `GET /v1/admin/diagnostics/memory-surfaces` also
reports the `built_in_skills` surface independently from filesystem `skills`,
including source, version, fingerprint, prompt policy, and model-facing tools.
The normal tool catalog reports `rusty_crew_help` in the mandatory `crew_help`
toolset.

## Verification

`npm run smoke:skills-tools` covers merge ordering, metadata, body truncation,
body omission, allowed-skill filters, missing roots, collisions, and immutable
mutation denial. `smoke:profile-loading`, `smoke:profile-role-assembly`,
`smoke:tool-profile-selection`, and `smoke:admin-diagnostics-api` cover profile
round trips, no filesystem leakage, prompt ordering/override behavior,
mandatory selection, and queryable health.
