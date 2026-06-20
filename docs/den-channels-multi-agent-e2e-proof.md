# Den Channels Multi-Agent E2E Proof

Den Channels is an adapter surface, not the Rusty Crew coordination bus. The coordination bus remains Rust-owned: channel ingress is normalized into Rust events and routed messages, while outbound replies and activity are projected back through the channel adapter.

## Scenario

`smoke:den-channels-e2e` proves the following end-to-end path:

- two agents have distinct channel bindings;
- two native Rust sessions are created for those agents;
- inbound Den Channels messages are accepted by per-binding transport controllers;
- accepted inbound messages are injected into Rust as external events;
- Rust routes each message to the correct agent and emits wake requests;
- duplicate message ids are dropped;
- stale cursors are dropped;
- reconnect retries resume the transport without replaying old messages;
- replies project to the correlated channel binding;
- completion activity/evidence projects to the correct binding;
- an outbound projection failure is degraded locally and does not block internal Rust agent-to-agent routing.

## Guardrails

The proof intentionally uses two bindings and two sessions. A success with only one agent would not catch the old centralized-adapter failure mode where channel state, cursors, or projection health accidentally become global.

The adapter owns transport cursors, dedupe, reconnect state, and projection delivery. Rust owns normalized external events, routed messages, wake policy, and durable coordination state.

## Verification Command

Run:

```bash
npm run smoke:den-channels-e2e
```

Expected output includes:

- `alphaRoute: "agent-alpha"`
- `betaRoute: "agent-beta"`
- routed event types containing `agent_message_routed` and `brain_wake_requested`;
- `duplicateDropped: true`;
- `staleDropped: true`;
- `internalRoute: true`.
