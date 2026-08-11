# Vision Playtester Worker Profile

The `vision_playtester` local tool profile turns Den's persistent Playwright
playtest lifecycle into a focused model-facing role. It is intended for
delegated final-product checks where an agent operates the ordinary visible
interface and reports evidence instead of repairing the product or its test
environment.

This profile is task-focus friction, not a security sandbox. The underlying
Den playtest CLI remains a permissive trusted-local debugging utility. Rusty
Crew exposes only four purpose-built operations to this profile:

- `playtest_start`
- `playtest_observe`
- `playtest_act`
- `playtest_finish`

The local profile also includes `deliver_completion_md`. It does not include
shell, filesystem, generic browser automation, arbitrary HTTP, configuration,
or application-internal tools. The `playtest_act` schema contains only genuine
keyboard, mouse, wheel, and wait actions. `playtest_observe` returns screenshots
or bounded frame bursts as actual model image inputs; it does not accept DOM,
storage, JavaScript expression, request, or CDP inspection fields.
Browser launch mode, video, and viewport remain manifest-owned rather than
model-selectable, so a failed start cannot turn into alternate harness setup.
Native Crew currently adds its immutable `rusty_crew_help` tool to every
profile. That help reader is the only extra model-facing tool and is not a
shell, browser, application-state, or mutation route.

## Service setup

Build or install the Den Services playtest CLI, then add its path and broker
configuration to the Rusty Crew service environment:

```dotenv
RUSTY_CREW_PLAYTEST_CLI=/absolute/path/to/den-playwright
RUSTY_CREW_PLAYTEST_CONFIG=/absolute/path/to/playwright-broker-config.yaml
```

For a source checkout of Den Services:

```bash
cd /home/dev/den-services/playwright-broker
go build -o /home/system/rusty-crew/bin/den-playwright ./cmd/den-playwright
```

Install the profile from
[`vision-playtester.json`](profile-templates/vision-playtester.json), adjusting
only installation-specific paths when necessary. Its default provider alias is
`deepseek-flash-responses`, which should resolve to the direct DeepSeek
Responses provider (`deepseek-v4-flash` in the currently certified setup).

The template deliberately has no whole-turn duration ceiling. It defaults to
delegation depth zero. Mission action, session-time, and estimated-cost budgets
are declared in the mission brief. The start adapter records the delegated
budget, counts attempted primitive actions, and rejects further act/observe
work after the action or session-time limit while keeping finish available for
an evidence-backed terminal report. The cost ceiling remains part of Crew's
delegated provider budget and final report validation; it is not approximated
from wall-clock time. None of these limits silently terminates a healthy Crew
turn.

`playtest_start` also requires `expected_revision`. Before launching, the
adapter compares that full commit SHA with the supplied repository root's
current Git commit. After launch, it reads the broker's authoritative evidence
index and requires `revision.commit_sha` to match again. A stale checkout,
non-Git harness root, unreadable index, or mismatch returns an
`infrastructure_error`-appropriate tool failure without claiming that mission
revision. The expected SHA is additionally retained in broker metadata; this
is evidence binding, not sandboxing.

## Mission contract

An orchestrator should provide:

- project and exact repository revision;
- project manifest path;
- short user-facing mission and controls;
- known startup expectations;
- explicit action, session-time, and estimated-cost budgets;
- artifact policy and Den/correlation handles;
- existing playtest session and evidence offsets when resuming.

`renderVisionPlaytestMission` provides a typed renderer for this packet. On
resume, reuse the same broker session ID. Do not start another browser session
merely because a provider request or Crew process was interrupted.

One or more workers can be delegated independently by using separate broker
sessions. A verifier run is another independent worker/session whose mission
references the original finding and evidence offsets. Ordinary Rusty Crew
delegation already persists lineage, completion packets, correlation IDs,
transcript tool results, provider continuation state, and resource limits.

## Outcome posture

The worker is an operator and observer, not a fixer. It may make at most one
bounded reproduction attempt when useful. It must not repair configuration or
services, invent alternate controls, deploy a replacement, inspect hidden
state, or broaden the mission. Concretely, a failed operation permits at most
one additional call for that operation; changing harness parameters does not
reset that count.

All of these are successful playtest completions when supported by evidence:

- `pass`
- `fail`
- `uncertain`
- `infrastructure_error`

Every finding should reference observation, action, timeline, screenshot,
frame, trace, or evidence-index offsets. A single screenshot is insufficient
as sole evidence for motion, handedness, collision, or state transition.
When calling `deliver_completion_md`, use Crew completion status `completed`
for any well-supported playtest outcome and put `pass`, `fail`, `uncertain`, or
`infrastructure_error` in the report body. Crew completion status is lifecycle
state, not the playtest verdict.

`validateVisionPlaytestReport` checks the report envelope, evidence references,
uncertainty explanation, provider/model identity, and declared action/cost
budgets. A playtester pass is review evidence, not merge authority.

## Verification

Deterministic CI uses:

```bash
npm run smoke:vision-playtester -w @rusty-crew/brain-island
npm run smoke:local-tool-profile-policy -w @rusty-crew/brain-island
```

The focused smoke covers exact catalog composition, absence of ordinary bypass
fields, image attachment, infrastructure-error reporting, same-session resume,
all four outcomes, stale-revision rejection, evidence requirements, and budget
diagnostics. Real model campaigns remain manual, scheduled, or
review-triggered.
