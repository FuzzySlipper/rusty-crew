# Telegram operator consults

The `request_telegram_consult` brain tool lets an exact active install-diplomat
session send a fresh message to its own bound Telegram chat or topic from any
ordinary wake. It is selected through the `telegram_diplomat` toolset and is
not part of the ordinary `full_agent` inventory.

Tool selection and delivery authority are separate. A profile may select the
toolset for several sessions, but Rust authorizes a call only when the calling
full session is the exact target of one active install-diplomat binding. The
tool accepts only a message and an optional reason category; bot, chat, topic,
binding, profile, agent, and session destinations are never model inputs.

Suggested profile guidance:

> If a remote technical consult would materially unblock you, especially for
> network trouble, an ambiguous user description, or unfamiliar machine state,
> use `request_telegram_consult` to send the operator a short summary of what
> you observed and the specific question. Continue safe local investigation
> when useful, and do not repeatedly send the same request.

Rust persists the request, calling session and profile, binding, originating
wake kind, optional category, delivery status, attempts, and Telegram message
IDs. Adapter failures settle the consult as failed but do not fail the local
brain turn. Pending records are revalidated against the current exact binding
and replayed when the Telegram connector starts after a service restart.

The initial policy is intentionally guidance rather than enforcement. Consult
diagnostics should be used to observe useful escalation, non-use, or repeated
noise before adding further behavior.
