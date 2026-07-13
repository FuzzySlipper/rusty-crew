# Cross-runtime direct messaging and correlated rounds

Run: `coordination-debug-1783918992360-62137418`
Scenario: `cross_runtime_agent_coordination`

## Runtime Results

| Runtime | Kind | Passed | Duration | Interactions | Recovery |
| --- | --- | --- | ---: | ---: | --- |
| direct-pi-agent | direct_brain | yes | 22617 ms | 2 | recovered |
| codex-app-server | codex_app_server | yes | 49854 ms | 2 | recovered |

## Unsupported Capabilities

- **direct-pi-agent:** none
- **codex-app-server:** none

## Validation

- `npm run smoke:coordination-live:debug -w @rusty-crew/capability-harness`
