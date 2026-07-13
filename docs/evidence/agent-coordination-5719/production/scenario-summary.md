# Cross-runtime direct messaging and correlated rounds

Run: `coordination-production-1783919070111-51c8aae4`
Scenario: `cross_runtime_agent_coordination`

## Runtime Results

| Runtime | Kind | Passed | Duration | Interactions | Recovery |
| --- | --- | --- | ---: | ---: | --- |
| direct-pi-agent | direct_brain | yes | 53591 ms | 2 | not_exercised |
| codex-app-server | codex_app_server | yes | 54216 ms | 2 | not_exercised |

## Unsupported Capabilities

- **direct-pi-agent:** none
- **codex-app-server:** none

## Validation

- `npm run smoke:coordination-live:production -w @rusty-crew/capability-harness`
