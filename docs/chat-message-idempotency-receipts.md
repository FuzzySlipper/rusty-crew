# Chat Message Idempotency Receipts

Status: implemented by task #5594

Chat message ingest uses two Rust-owned persistence records with distinct
lifetimes:

- `chat_message_ingest_receipts` is the typed reservation/finalization record.
  Reservation, slot creation, and receipt finalization occur in one database
  transaction. Finalized receipts expire after 30 days and may be pruned.
- `message_slots.ingest_idempotency_key` is the durable anti-resurrection key.
  It is unique within a session and remains for the lifetime of the message
  slot.

After a finalized receipt expires, replay first resolves the durable slot key.
It returns the original slot as a duplicate and cannot create a replacement
message. Receipt expiry therefore bounds coordination-only receipt storage
without making an old idempotency key reusable.

Receipt maintenance accepts an explicit RFC 3339 cutoff. Normal ingest also
prunes expired finalized receipts for its session. Reserved receipts are never
pruned: they exist only inside the same uncommitted transaction as slot
creation, so a crash rolls the reservation back.

Session/profile hard deletion removes both the receipt rows and the durable
message slots. Reusing an idempotency key is valid only after that explicit
destructive boundary. Deleting a receipt alone never makes a key reusable.

SQLite and PostgreSQL implement the same table, unique slot-key constraint,
transaction ordering, and maintenance operations. There is no generic
`simple_kv` fallback and current test data is not migrated from the former
`chat_message_ingest` scope.
