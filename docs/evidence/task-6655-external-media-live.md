# Task 6655 external-runtime live media proof

Date: 2026-08-09 (America/Los_Angeles)

Implementation revision: `ece319ac9f024f6ccb2bdfa2a8160d599b0686fe`

Service: `rusty-crew-debug.service`, API port `9348`, runtime
`codex-debug-app-server` (`desiredState=enabled`, `observedState=ready`).

## Disposable session

- binding: `external-binding-00b319cf0789f6c83651a01a`
- session: `external-session-00b319cf0789f6c83651a01a`
- native thread: `019fe5f8-70bf-77c1-8fd8-8d81d7a5b144`
- native turn: `019fe5f8-e842-7ee0-a9f5-f4303a3f692b`

The turn was asked to inspect two existing PNGs with the image-view tool and
then run `sleep 30` before returning its final response. This made the
in-progress publication boundary directly observable.

## Ordered publication

The normalized runtime event API published the first image on item-started
sequence 159 at `2026-08-09T10:02:06.950Z` and the second on item-started
sequence 165 at `2026-08-09T10:02:09.504Z`. The turn completed at sequence 262
at `2026-08-09T10:02:54.447Z`. Both media checkpoints were therefore visible
before completion, in the same order as the runtime's image-view actions.

| Sequence | Filename | Size | Dimensions | SHA-256 |
| --- | --- | ---: | ---: | --- |
| 159 | `den-planning-turn.png` | 292135 | 1280x720 | `97b10da3e56871f6645a62ab16e25d928fd3e63d6add1330b13b4849482c6647` |
| 165 | `memory-boundary-both-turns.png` | 299696 | 1280x720 | `470b45eb41ce7f7748ea3234b59b28c7fcea3981b9af1bfb335b90f8f3f30145` |

The event payloads contained opaque application content URLs and attachment
metadata. They did not contain source host paths or image bytes.

## Content delivery and restart

Each content URL was fetched through both `http://127.0.0.1:9348` and the
LAN-bound `http://192.168.1.22:9348` endpoint. A second machine, `den-nimo`,
also fetched the first opaque content URL from the LAN endpoint without access
to its originating host path; it received 292135 bytes with the expected
`97b10d...c6647` SHA-256. The application responses supplied
`content-type: image/png`, authoritative content length, SHA-256, dimensions,
and quoted `sha256:` ETags. An `If-None-Match` request returned 304.

The debug service was restarted onto the implementation revision at
`2026-08-09 03:05:04 PDT` (PID 1944661). After restart, event sequences 159 and
165 retained the same media references, both content URLs returned the same
hashes, and the conditional request still returned 304.

## Automated verification

- `npm run test:unit --workspace @rusty-crew/external-runtime-codex`
- focused brain-island controller, route, and attachment tests
- `npm run typecheck`
- `npm run smoke:external-runtime-api-contract`
- `npm run verify:offline`
- `npm run verify:ts`
- `npm run test:postgres-backend`

The PostgreSQL conformance run completed with 39 persistence tests and the
review-submission restart/revision test passing.
