# Memory And Den Document Boundary Live Certification

Status: certified for task 4279
Date: 2026-07-25

This note records the repeatable live path for the `asha-planner` style failure
mode where an agent asks for Den documents or tasks but reaches for memory tools
instead.

## Target

Use the disposable debug service unless the task explicitly needs live-service
state:

```bash
export CREW=${RUSTY_CREW_DEBUG_ADMIN_BASE_URL:-http://127.0.0.1:9348}
export RV_LIVE_BACKEND_URL="$CREW"
```

The debug service must have:

- a live provider alias suitable for the profile under test;
- a profile with Den MCP bindings that expose document, task, and guidance
  tools;
- external memory configured according to the test case:
  - healthy when testing memory behavior;
  - intentionally missing/unhealthy when testing pre-wake omission and
    diagnostics.

## Deterministic Guard

Run:

```bash
npm run smoke:memory-document-boundary -w @rusty-crew/brain-island
```

The smoke proves:

- unavailable external memory omits `memory_recall`, `memory_read`,
  `memory_search`, `memory_store`, and `memory_propose` before wake;
- Den document/task/guidance MCP tools remain selected when external memory is
  unavailable;
- Crew-owned dense profile memory, memory-space browsing, session search, and
  roleplay lore stay distinct;
- no selected model-facing tool uses the old `den_memory_*` shape.

## Live Prompt

Use a profile such as `asha-planner` that has both Den MCP planning tools and
external memory tools available through its selected profile/tool bindings.

```text
Read Den docs asha / entity-composition-model-design-question and asha /
ecrp-vocabulary-taxonomy and asha / ecrp-fps-ownership-matrix. Then list the
current open tasks in project asha that are relevant to those docs. I want to
examine whether we have properly implemented these concepts into the asha engine
enough. Do not use memory tools for Den docs or Den tasks; use memory tools only
if you need remembered background that is not a Den document or Den task.
```

Expected behavior:

- the agent uses Den MCP document tools for the three document reads;
- the agent uses Den MCP task/list/search tools for current task lookup;
- memory tools are unused for the document/task lookup path;
- if external memory is unavailable, memory tools are absent or clearly
  reported as omitted before wake rather than failing mid-turn;
- the final answer discusses the requested Asha concepts and does not collapse
  Den documents into memory.

## Evidence To Capture

For Rusty View certification, follow `docs/live-deliverable-certification.md`
and `../rusty-view/docs/live-testing.md`.

The task-thread evidence should include:

- backend URL and profile id;
- provider alias and brain module reported by `/model`;
- transcript screenshot or artifact path;
- tool/activity stream showing document/task MCP calls;
- tool-selection diagnostics proving external memory status before wake;
- any provider request debug-cache refs needed to inspect prompt/tool assembly;
- whether external memory was healthy, omitted, or intentionally unavailable.

If the live prompt still reaches for memory tools for Den docs/tasks, leave the
task in progress or blocked and attach the failed transcript/tool evidence. Do
not call the boundary certified until a real profile completes the prompt with
the expected tool families.

## 2026-07-08 Debug-Service Certification

- Backend: `http://127.0.0.1:9348`
- Storage: SQLite debug service
- Profile/session:
  `asha-planner` /
  `asha-planner-session-20260706T09140884-1`
- Provider/brain: `deepseek-flash` through `pi-agent-core`
- Context diagnostics before wake: `tool_count=113`, `mcp_binding_count=1`,
  `mcp_active_count=1`, `degraded=false`
- Wake:
  `service-asha-planner-session-20260706T09140884-1-1783514649663-3`
- Prompt message id: `task-4708-memory-doc-boundary-cert-1`
- Event cursor range:
  `asha-planner-session-20260706T09140884-1:7661` through
  `asha-planner-session-20260706T09140884-1:10785`

Observed tool-call starts:

| Tool | Starts |
| --- | ---: |
| `den_get_document` | 8 |
| `den_search_documents` | 4 |
| `den_list_tasks` | 32 |

Observed outcome:

- no `memory_*` tool starts appeared in the certification event range;
- the first three tool calls were `den_get_document`, matching the requested Den
  docs;
- task lookup used `den_list_tasks` and document discovery used
  `den_search_documents`;
- the wake completed with `assistant_message_completed.status=completed` and
  summary `LLM wake completed.`;
- the final answer assessed ECRP implementation status in the Asha engine rather
  than reporting tool unavailability or treating Den docs as memory.

This certifies the debug-service path for the original confusing
`asha-planner` prompt shape. Rusty View rendered-screenshot certification is
still the higher-level UI evidence path when the frontend display itself changes.

## 2026-07-25 Rusty View Certification

The full rendered path was rerun through the Rusty View Playwright broker
against the SQLite debug service and a live `deepseek-flash` provider.

- Backend: `http://127.0.0.1:9348`
- Profile/session:
  `asha-planner` / `asha-planner-session-20260725T08202534-1`
- Brain/provider: `chat-completions` / `deepseek-flash`
- Den planning binding: profile-scoped `den` MCP server with tool profile
  `planner`
- External memory: metadata-mode adapter pointed at a bounded certification
  endpoint that returned one unique stored marker
- Broker run:
  `rusty-view-20260725T082043.011595254Z-2764929`
- Result: one Chromium scenario passed in 11.8 seconds with no browser console
  errors or page errors

Observed routing:

| Prompt half | Completed model-callable tools | Excluded route |
| --- | --- | --- |
| Den document and task lookup | `den_get_document`, `den_get_task` | no external-memory tool calls |
| External-memory marker recall | `memory_recall` | no `den_*` tool calls |

The external-memory endpoint received exactly one request at
`/v1/memories/recall`. The response marker was not present in the prompt, and
the rendered assistant answer returned it only after the successful tool call.

Durable evidence:

- Rusty View task `#6189`, scenario
  `apps/rusty-view-e2e/src/live/memory-boundary-cert.live.spec.ts`, committed at
  `bb1565009284a9a4a2adab9126de7cd89faf8cbf`
- [`den-planning-turn.png`](evidence/task-4279/den-planning-turn.png)
- [`memory-boundary-both-turns.png`](evidence/task-4279/memory-boundary-both-turns.png)
- [`visible-transcript.txt`](evidence/task-4279/visible-transcript.txt)
- [`evidence-packet.json`](evidence/task-4279/evidence-packet.json)

The certification also found and fixed three real rebuild/configuration gaps:

1. runtime brain rebuilds did not receive service adapter factories;
2. rebuilt brain executors did not receive those factories when resolving
   model-callable tools;
3. explicit `undefined` path overrides erased external-memory default paths.

After those fixes, the profile was hot-swapped again before the passing run so
the proof covers the rebuild path rather than startup-only behavior.
