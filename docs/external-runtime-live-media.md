# External Runtime Live Media

Rusty Crew treats images deliberately presented by an external runtime as
ordered transcript evidence. It does not infer evidence from general file
activity.

## Capture boundary

The Codex adapter extracts transient media candidates from:

- dynamic-tool `contentItems` with `type: inputImage`;
- MCP result content with `type: image`;
- `imageView` items, whose path is read immediately as a fallback.

Candidate bytes and host paths are consumed before the normalized event is
written. Raw-detail projection replaces image bytes and `imageView` paths with
redaction markers. Durable event payloads contain only `media` references with
capture state, attachment identity, safe filename, MIME, size, SHA-256,
dimensions, and an application content URL.

## Authority and identity

External images reuse `ToolMediaAttachmentStore`, the existing Crew attachment
metadata authority, artifact directory, and chat attachment content route. No
second blob store is introduced.

The stable attachment identity includes runtime, binding/session, native
thread, native turn, item, and media index. Replaying one native item therefore
reuses the same attachment. A later item or turn creates a distinct checkpoint,
even if its filename is unchanged. Duplicate bytes from two different input
seams within one item collapse to one reference; repeated positions from the
same structured result remain ordered presentation entries.

Bytes are persisted before an `available` reference is written. Other durable
states are `unavailable`, `unsupported`, `empty`, `oversized`, and `failed`.
An `imageView` source that changes during the read is recorded as unavailable
instead of publishing uncertain bytes.

## Content delivery

The existing attachment content route returns the authoritative MIME and byte
length. External media also supplies a SHA-256 ETag, `x-content-sha256`, and
image dimension headers. Conditional reads using `If-None-Match` return 304.
The opaque content URL works for a second LAN client through the application;
it never contains the originating host path.

## Current bounds

- Maximum image size: 20 MiB.
- Accepted MIME families: PNG, JPEG, GIF, and WebP.
- Dimension validation currently recognizes PNG, JPEG, GIF, and extended
  WebP (`VP8X`) headers. Other WebP encodings fail with a typed capture state.
- Video and audio are outside this media-first campaign slice.
