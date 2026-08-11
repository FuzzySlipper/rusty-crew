# Task 6785 Vision Playtester Live Evidence

## Scope

- Rusty Crew service: disposable SQLite debug service at
  `http://127.0.0.1:9348`
- profile: `task-6785-vision-playtester-cert`
- Crew session: `task-6785-vision-playtester-cert-session`
- provider: `deepseek-flash-responses`
- model: `deepseek-v4-flash`
- target: `/home/dev/rusty-engine-demo`
- exact target revision: `2a0f9dc60209b1c2a780a40241ee9c9f07ff5f1b`
- target server: an already-running `den-serve` session at
  `http://127.0.0.1:37300`

The task implementation was loaded directly from the Rusty Crew task checkout;
the exact submitted implementation revision is recorded in the Den delivery.
The debug service alone received the trusted-local playtest CLI environment.
The production service was not restarted or reconfigured.

## Profile and boundary readback

The live context reported local tool profile `vision_playtester`, zero MCP
bindings, and six model-facing tools. Five are profile-purpose tools:
`playtest_start`, `playtest_observe`, `playtest_act`, `playtest_finish`, and
`deliver_completion_md`. The sixth is Crew's mandatory immutable
`rusty_crew_help` reader. There was no shell, filesystem, generic browser,
arbitrary HTTP, eval, CDP, hidden-state, or application-mutation route.

This catalog is focus friction, not a security sandbox. The underlying Den
playtest broker remains a permissive trusted-local utility.

## Exploratory run and correction

The first exploratory mission selected headed video startup, retried the failed
startup by changing launch parameters, and then exceeded its declared action
budget while trying to locate the menu. The orchestrator cancelled the Crew
turn and broker session. This produced two implementation corrections before
certification:

1. headed mode, recording, and viewport became manifest-owned rather than
   model-selectable;
2. the adapter began recording the delegated budget at start, counting every
   attempted primitive action, rejecting over-action-budget input and
   over-session-time act/observe calls, and preserving finish for a supported
   terminal report.

The prompt also makes one reproduction literal: one additional call for a
failed operation, without resetting the count through alternate parameters.

## Passing WebGL mission

The certification mission supplied only the visible initial `NEW GAME` button
coordinate, leaving the worker to explore the game surface with ordinary input.
It passed an eight-action, eight-minute, USD 0.25 estimated-cost budget to
`playtest_start` unchanged.

Broker session:
`task-6785-rusty-engine-demo-playtest-20260811T035917.992786147Z-401472`

Evidence index:
`/home/agent/.cache/den-playwright/runs/task-6785-rusty-engine-demo/task-6785-rusty-engine-demo-playtest-20260811T035917.992786147Z-401472/playtest-index.json`

Observed sequence:

1. `screenshots/0001-initial-new-game-screen.png` showed the ordinary Loading
   Bay menu and visible `NEW GAME` control.
2. A genuine mouse click at `(497, 484)` changed the route from `#/` to
   `#/game?mode=new`, changed the title to `Rusty Engine — Loading Bay`, grew
   request count from 7 to 64, opened one WebSocket, and produced
   `screenshots/0002-after-new-game-click.png`.
3. A genuine surface click at `(640, 400)` changed pointer lock from `null` to
   `canvas`.
4. Holding `W` for 900 ms produced the stable six-frame series
   `screenshots/0003-frame-burst-after-w-input-{1..6}.png`.
5. Holding `A` for 900 ms produced the separately stable six-frame series
   `screenshots/0004-frame-burst-after-a-input-{1..6}.png`. Visual inspection
   confirms that the camera position differs materially from series 0003.
6. The adapter reported exactly eight primitive actions used and zero actions
   remaining. The model issued no further input and finalized the broker with
   outcome `pass`.

No page errors were recorded. The final report named the actual provider,
model, broker session, evidence index, action count, and artifact offsets. It
also retained a low-uncertainty caveat rather than claiming pixel-perfect
causality from one screenshot.

## Deterministic coverage

`smoke:vision-playtester` covers actual profile composition (including
mandatory help), absence of bypass schemas, image attachment, structured
infrastructure errors, same-session resume after provider interruption, action
and session budget enforcement with finish still available, all four outcomes,
evidence requirements, and cost-budget report diagnostics.

The profile has no whole-turn duration ceiling and defaults delegation depth to
zero. Per-operation CLI timeout and explicit mission budgets remain bounded;
healthy provider/tool round counts are not capped.
