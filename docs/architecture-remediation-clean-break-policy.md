# Architecture Remediation Clean-Break Policy

Status: active for the architecture remediation task series

Date: 2026-07-01

## Purpose

The current local Rusty Crew service data is test data. During the architecture
remediation series, correctness and long-term structure are more important than
preserving scratch profiles, sessions, provider records, bindings, or local
chat transcripts.

This policy exists so implementation agents remove stale fallback paths instead
of keeping them because they happen to preserve the current dev database.

## Policy

Architecture remediation may reset or recreate local Crew service data.

Agents should prefer:

- a clean official create/import/setup path;
- documented reset/recreate commands;
- typed API/UI configuration flows;
- repeatable live-test profile/provider/tool-profile setup.

Agents should not add or keep:

- old-shape config fallback reads;
- duplicated bridge operations just for old clients;
- file/DB mixed active prompt fallbacks after DB-backed prompt fields are the
  chosen path;
- stale package import shims after packages are moved;
- route aliases used only by historical tests;
- migration code whose only purpose is to preserve current scratch data.

## What May Be Reset

During this remediation window, these may be discarded and recreated:

- local sessions and chat history;
- local profile registry records;
- local model-provider aliases;
- local tool profile records;
- local MCP/channel bindings;
- local provider wire state;
- dense/profile memory and roleplay/lore test data;
- runtime counters, telemetry, search indexes, and diagnostics.

If a test needs any of this state, the test setup should create it through the
official API or setup script.

## What Must Still Be Designed For Future Real Data

Clean-break policy for current test data does not remove future portability
requirements. Real deployments still need:

- logical export/import records;
- dry-run validation;
- idempotent import batches;
- explicit queue no-resurrection checks;
- backend capability checks;
- operator runbooks for quiesced/read-only migration windows when needed;
- profile/provider/tool-profile export/import for backup and review.

Do not confuse "we can reset this dev instance" with "the product never needs a
migration story."

## Provider Secret Handling

Live-test provider aliases may be recreated freely. They should point at local
or LAN provider/router endpoints that handle secrets outside the repository.

Do not commit API keys or copied secrets into repo docs, profile exports, or
test fixtures.

## Live Testing

After structural changes that affect chat/runtime behavior, recreate the minimal
live-test setup and certify behavior through Rusty View's live testing
framework. Deterministic tests and smokes remain necessary, but they do not
replace rendered chat evidence for user-facing features.

