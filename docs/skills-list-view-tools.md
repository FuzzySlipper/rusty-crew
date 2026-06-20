# Skills List And View Tools

Task: Den `2902`

Rusty Crew now has read-only brain-island skill inspection tools:

- `skills_list`
- `skill_view`

They build on the existing profile/skill loader rather than creating a second
skill registry.

## Behavior

`skills_list` reads configured skill roots and returns skill metadata:

- slug
- title
- summary
- tags
- source path
- status
- parse error, when invalid metadata is included

By default invalid skills are hidden from the list. Passing `includeInvalid`
includes them with status `invalid` and an error message.

`skill_view` reads one skill by safe slug. It can include or omit the body and
supports body truncation through `maxBodyChars`.

## Safety

The tools are read-only. `skill_view` accepts a slug, not an arbitrary path, and
slugs must match a safe filename pattern. If `allowedSkills` is supplied in the
tool context, view/list operations are restricted to that profile-visible set.

Missing or invalid skill roots return structured tool results with reason codes
instead of throwing raw filesystem errors into the model turn.

## Verification

`npm run smoke:skills-tools` covers valid skills, invalid frontmatter, hidden
invalid entries, safe slug lookup, body truncation, body omission,
profile-scoped denied skills, and missing skill roots.
