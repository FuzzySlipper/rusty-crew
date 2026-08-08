# External Codex Thread Lineage

Rusty Crew treats an external binding as the durable identity edge between one
Crew session and one native Codex thread. A newly empty native thread is never
silently substituted into an existing binding.

## Replacement contract

Managed `/new`, `/restart`, profile prompt refresh, and dynamic tool catalog
refresh create a distinct full Crew session and external binding. The successor
binding records `lineage` with:

- the predecessor binding, Crew session, and native thread IDs;
- the idempotent transition ID and stable reason code;
- the predecessor revision timestamp used by the transition.

The predecessor binding continues to name the predecessor native thread. An
explicit operator `/new` or `/restart` retains the predecessor as active and
readable. Automatic profile or tool refresh archives the predecessor after the
successor is durable, but does not delete it. Actual deletion remains a separate
explicit lifecycle operation.

`thread_lineage_replaced` is the durable diagnostic event. Its payload names
both sides, the transition reason, whether the predecessor was retained or
archived, and the number of curated routes moved. Native `thread/started`
notifications that do not match a binding are recorded as unbound lifecycle
events; they do not retarget a Crew session.

## Routing and recovery

Curated managed-external routes that exactly target the predecessor are moved
to the successor with their expected route revisions. Other bindings and routes
are not changed. Successor creation uses a deterministic idempotency key, and
the lineage record is validated on replay, so reconnect and service restart
recover the same successor rather than creating another one.

The predecessor and successor remain independently listable and readable. The
successor begins with an empty native transcript unless a future replacement
contract explicitly proves transcript continuity.

## Workspace boundary

The successor inherits the predecessor session's working directory as execution
context. This contract does not add profile roots, exclusions, allowed paths, or
filesystem restrictions. A full session working directory is not a confinement
boundary, and absolute paths outside it remain governed by the surrounding
harness and environment. Separately typed delegated/subagent workspace
confinement is outside this contract.
