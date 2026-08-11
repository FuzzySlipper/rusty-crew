# External thread history pagination

`POST /v1/external-runtimes/{runtime_id}/threads/read` reads native thread
metadata without native turns and hydrates at most one Rust-owned durable turn
page. `includeTurns: true` means “include one bounded page”; it never means
“serialize the complete Codex transcript.”

The default page limit is 50 and the maximum is 100. The first read returns the
most recent page in chronological order. When `turnPage.hasMoreBefore` is true,
send `turnPage.beforeCursor` as `beforeCursor` to read the immediately older
page. Cursors bind the runtime, native thread, immutable turn creation time, and
request identity. A different thread, malformed cursor, or missing cursor turn
is an explicit 400/409 error rather than an empty or reset transcript.

Each turn and item has stable Crew/native identity. Item text, reasoning
summaries, and item counts are bounded; `truncated`, `itemsTruncated`, and
`detailHandle` expose that distinction. Media and input images remain separate
attachment URLs. Live events can therefore merge by turn/item identity while
older pages load without moving the page boundary when a new turn arrives.

Small sessions keep the familiar shape: `thread.turns` contains all turns when
the durable history fits in one page. As a compatibility bridge for an old or
imported small native thread that has no Crew turn index, a bounded native
metadata response containing no more than the requested limit may still be
projected. Larger unindexed native histories are never silently serialized.

Archived native transcripts remain readable through the same bounded contract;
archive and other lifecycle operations do not require a history read.
