# Crew-services direct-brain adapter

The first assembled adapter is intentionally local: one loopback `crew-messaging`
binary, one Rusty Crew service, and explicit direct-brain aliases. It does not
modify the deployed service, existing sessions, or managed Codex sessions.

Start the neutral fabric on the agent box:

```sh
cd /home/dev/crew-services
go run ./cmd/crew-messaging \
  -db /home/system/crew-services/crew-messaging.db \
  -listen 127.0.0.1:8787
```

Enable the Rusty Crew service adapter with deliberate, revisioned bindings:

```sh
RUSTY_CREW_CREW_SERVICES_ENABLED=true
RUSTY_CREW_CREW_SERVICES_URL=http://127.0.0.1:8787
RUSTY_CREW_CREW_SERVICES_ADAPTER_ID=rusty-crew-fabric
RUSTY_CREW_CREW_SERVICES_INSTANCE_ID=agent-k8
RUSTY_CREW_CREW_SERVICES_LEASE_DURATION=2m
RUSTY_CREW_CREW_SERVICES_RENEW_MS=45000
RUSTY_CREW_CREW_SERVICES_POLL_MS=1000
RUSTY_CREW_CREW_SERVICES_CLAIM_DURATION=45s
RUSTY_CREW_CREW_SERVICES_BINDINGS_JSON='[
  {"alias":"alpha","routeKey":"@alpha","routeRevision":1},
  {"alias":"beta","routeKey":"@beta","routeRevision":1}
]'
```

The URL, adapter/instance identity, and each alias route revision are all
required when enabled. Disabled is the default and creates no fabric client,
poller, or crew tool surface. `crew_directory` and `crew_message` appear only
for currently exact-bound direct-brain sessions; rebind/archive/external drift
removes that visibility on the next adapter refresh and each action revalidates.

For a disposable real-boundary check, from `/home/dev/rusty-crew` run:

```sh
npm run smoke:real-boundary -w @rusty-crew/adapter-crew-services
```

It creates temporary SQLite/data directories and scratch loopback port, builds
a disposable local Go binary inside that temporary root, creates two temporary direct-brain routes through the real
native bridge, proves ordinary/replay/linked-reply fabric translation and
inspection, then stops and removes everything. It owns the direct child and
uses bounded TERM then KILL cleanup before removing its root. `CREW_SERVICES_BIN` may point to
a prebuilt binary and `CREW_SERVICES_DIR` may override the source directory.

The current assembled proof intentionally does not certify busy-to-idle UI
notification timing, fault-injected begin/restart cases, or managed-Codex
delivery. Those mutation ambiguity and FIFO cases stay in the focused adapter
tests; managed Codex remains the second slice because Rusty Crew’s external
controller remains its authority.
