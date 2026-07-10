# Roleplay ST Evaluation Harness

Status: initial deterministic harness implemented.

The harness lives in `ts/packages/brain-island/src/roleplay-st-example-fixture.ts`
and uses `/home/stash/st-example/` as a reference corpus. It does not try to
regenerate the full SillyTavern transcript. The first useful scenario is the
first generated assistant response after the static opening and first user
reply.

## Entry Points

- `npm -w @rusty-crew/brain-island run smoke:roleplay-behavior-eval`
- `npm -w @rusty-crew/brain-island run smoke:roleplay-st-import-service-api`

The shared fixture module exposes:

- `loadStExampleFixture()`
- `buildStExampleImportPlan()`
- `firstResponseScenario()`
- `evaluateFirstResponse()`

## Rubric

The deterministic rubric checks broad behavior instead of exact text equality:

- Xavier remains the assistant viewpoint.
- The political cover story remains in view.
- The Kopis/Xavier bodyguard intimacy dynamic is recognized.
- The prose keeps an elevated dark-fantasy/courtly register.
- The response continues from the first user reply.
- The output has no labels, JSON fences, tool-call artifacts, or debug text.

`evaluateFirstResponse()` returns a report with scores, notable misses, optional
prompt stack trace, optional lore evidence, and the response text. Live model
comparison can layer on top of this report shape and should remain manual or
environment-gated.
