# Retired Whole-Wake Timeout Evidence

Task #4478 originally certified an elapsed-time whole-wake deadline. Task #6372
removed that behavior, its configuration fields, admin route, bridge state,
diagnostics, and terminal reason path.

This file remains only as a historical task pointer. Current logical turns have
no finite service/profile/session lifetime. Use scheduling quanta for durable
yield and continuation, progress-aware operator attention for repeated
no-progress, and explicit cancellation when an operator intends to stop a turn.

Current certification is documented in
`docs/logical-turn-continuation-live-certification.md`.
