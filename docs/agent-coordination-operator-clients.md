# Agent Coordination Operator Clients

Rusty Crew exposes separate local operator clients for the production and debug
deployments. They list same-service recipients, send one TTL-bound message, or
start and wait for one durable correlated round.

The production client has no endpoint, port, service URL, deployment selector,
or debug switch:

```bash
npm run agent:coordination -- list
npm run agent:coordination -- send <agent-id> <ttl-seconds> <message...>
npm run agent:coordination -- round <agent-id> <ttl-seconds> <message...>
```

Debug communication is a separately named command:

```bash
npm run agent:coordination:debug -- list
npm run agent:coordination:debug -- send <agent-id> <ttl-seconds> <message...>
npm run agent:coordination:debug -- round <agent-id> <ttl-seconds> <message...>
```

The commands read only their fixed service roots:

- production: `/home/system/rusty-crew/config/service.env`;
- debug: `/home/system/rusty-crew-debug/config/service.env`.

`RUSTY_CREW_DEPLOYMENT_ROLE` must be `production` or `debug`. The client checks
the local file and the service response. The production API lives under
`/v1/coordination/*`; debug lives under `/v1/debug/coordination/*`. Calling a
route on the wrong deployment fails with
`coordination_deployment_role_mismatch`.

TTL is required by the CLI, is expressed in whole seconds, and is bounded to
1-300 seconds. Expired deliveries and rounds remain terminal and are never
resurrected. JSON output identifies the deployment role, target agent, delivery
ID, round ID, status, and terminal reason. A correlated operator round includes
a service-authored reply contract so direct and managed Codex recipients can
return through their native Crew coordination tool.
