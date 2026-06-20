# Skill Manage Governance

Task: Den `2903`

Rusty Crew now exposes `skill_manage` in the brain-island skill tool module.
The implementation follows the current Rusty Crew skill format: a skill is a
profile-scoped `slug.md` file under the configured skills root. Optional support
files live under a narrow sidecar directory, `slug.d/`.

## Authority

`skill_manage` is profile-policy controlled. The tool exists as an exported
implementation, but `resolveSkillsTools` only includes it when the runtime
context sets `manageMode` to `profile` or `curator`.

Modes:

- `off`: default. All management calls are denied with
  `skill_manage_disabled`.
- `profile`: ordinary governed management for profiles that intentionally
  request the `skills_manage` toolset.
- `curator`: reserved for stricter governance profiles. The context must also
  set `curatorApproved`.

This keeps skill writes out of the default read-only skill surface.

## Actions

Supported actions:

- `create`: writes a new `slug.md`; existing skills are never overwritten.
- `patch`: either full-content replacement or unique `old_string` replacement.
- `write_file`: writes support files only below `slug.d/references`,
  `slug.d/templates`, `slug.d/scripts`, or `slug.d/assets`.
- `delete`: removes the active skill by archiving it under `.archive`.

Delete requires an explicit `absorbed_into` field. It may be an empty string
when the skill is being pruned without a replacement, but the caller has to make
that intent explicit.

## Safeguards

The tool rejects unsafe slugs and path traversal. If `allowedSkills` is supplied
in the tool context, mutation is limited to those profile-visible slugs.

Pinned deletion is denied. Rusty Crew checks these marker locations:

- `slug.pinned`
- `slug.d/.pinned`
- `slug/.pinned`, for compatibility with older directory-shaped skill stores

Deletes archive the active `slug.md` and any `slug.d` sidecar into `.archive`
with a timestamped delete manifest containing `absorbed_into` and optional
`provenance`.

## Verification

`npm run smoke:skills-tools` covers:

- management denied by default
- resolver inclusion when `manageMode` is enabled
- create without overwrite
- patch replacement and non-unique patch denial
- support-file writes in `slug.d`
- path traversal denial
- pinned delete denial
- required `absorbed_into`
- archive-on-delete behavior
