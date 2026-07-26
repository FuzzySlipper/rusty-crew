# Agent Coordination Operator Clients

Rusty Crew exposes separate local operator clients for the production and debug
deployments. They list same-service recipients and curated switchboard routes,
send one TTL-bound message, or start and wait for one durable correlated round.

Curated route addresses use an explicit `@<route-key>` grammar, such as
`@reviewer`. A route is a revisioned database record pointing to one exact
agent/session or one exact managed external binding revision. Crew never
resolves display labels, profile IDs, or thread names as addresses.

The production client has no endpoint, port, service URL, deployment selector,
or debug switch:

```bash
npm run agent:coordination -- list
npm run agent:coordination -- routes
npm run agent:coordination -- send <@route-or-agent-id> <ttl-seconds> <message...>
npm run agent:coordination -- round <@route-or-agent-id> <ttl-seconds> <message...>
```

Debug communication is a separately named command:

```bash
npm run agent:coordination:debug -- list
npm run agent:coordination:debug -- routes
npm run agent:coordination:debug -- send <@route-or-agent-id> <ttl-seconds> <message...>
npm run agent:coordination:debug -- round <@route-or-agent-id> <ttl-seconds> <message...>
```

The commands read only their fixed service roots:

- production: `/home/system/rusty-crew/config/service.env`;
- debug: `/home/system/rusty-crew-debug/config/service.env`.

`RUSTY_CREW_DEPLOYMENT_ROLE` must be `production` or `debug`. The client checks
the local file and the service response. The production API lives under
`/v1/coordination/*`; debug lives under `/v1/debug/coordination/*`. Calling a
route on the wrong deployment fails with
`coordination_deployment_role_mismatch`.

## Switchboard API

Use the role-specific prefix shown above. Production examples are:

- `GET /v1/coordination/routes` lists routes with current resolution status;
- `POST /v1/coordination/routes` creates a route;
- `GET /v1/coordination/routes/{routeKey}` reads one route and resolution;
- `PATCH /v1/coordination/routes/{routeKey}` replaces one route using
  `expectedRevision`;
- `DELETE /v1/coordination/routes/{routeKey}?expectedRevision=N` deletes one
  route;
- `POST /v1/coordination/routes/resolve` resolves an `address` without sending;
- `POST /v1/coordination/routes/{routeKey}/test` sends a bounded test message.

The debug forms use `/v1/debug/coordination/...`. There is no deployment
selector in any route payload.

A direct-brain target is:

```json
{
  "type": "direct_brain",
  "agentId": "review-agent",
  "sessionId": "review-session"
}
```

A managed external target is:

```json
{
  "type": "managed_external",
  "agentId": "review-agent",
  "bindingId": "review-binding",
  "bindingRevision": 4
}
```

Writes also accept `routeKey`, `label`, optional `description`, `enabled`,
optional `requiredRuntimeKind`, optional `requiredDeliveryPolicy`, and, for an
update, `expectedRevision`. Set `requiredDeliveryPolicy` to
`serial_next_turn` for a review queue. Missing, disabled, archived, replaced,
or policy-mismatched targets fail closed. Accepted delivery receipts preserve
the requested address, route key and revision, and resolved concrete target.

TTL is required by the CLI, is expressed in whole seconds, and is bounded to
1-300 seconds. Expired deliveries and rounds remain terminal and are never
resurrected. Once a message is accepted, that TTL no longer limits its one-reply
return path: Crew retains the exact sender agent/session identity, and a later
reply remains valid while that same sender session is active. The reply carries
its own delivery TTL. No switchboard route for the sender is required.

JSON output identifies the deployment role, target agent, delivery ID, round
ID, status, and terminal reason. Message and round writes use the `toAddress`
field. A correlated operator round includes
a service-authored reply contract so direct and managed Codex recipients can
return through their native Crew coordination tool.
