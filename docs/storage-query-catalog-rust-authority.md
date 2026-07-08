# Storage Query Catalog Rust Authority

Status: implementation note for task 4712

The admin storage query catalog at `GET /v1/admin/storage/query-catalog` is a
Rust bridge read model. TypeScript still owns route envelopes, parameter
presentation, UI copy, pagination, and redaction, but module-owned query
authority comes from `storageSchema()`.

## Module-Owned Queries

Module query entries are read from
`NativeRuntimeModuleSchemaRegistryDiagnostics.modules[].queryCatalogEntries`.
The current mapped module query is:

- Rust module query `simple_kv.list_entries_by_scope`
- Public storage query id `simple_kv.entries`
- Logical store `simple_kv.entries`

The catalog copies Rust-owned module metadata into the public descriptor:

- module id and schema version;
- Rust owner crate/module;
- logical store id and description;
- Rust query id and parameter schema id;
- backend capability list and per-capability support status.

If Rust adds a module query entry and TypeScript has no execution mapping, the
catalog route fails with `unmapped_rust_module_query`. That is intentional: a
new Rust query should not appear as a silently unusable or TS-invented storage
tool.

## System Queries

Runtime/system queries such as `storage.schema`, `storage.table_counts`,
`runtime.search`, `profile.memory`, `conversation.branches`, and
`runtime.counters` remain TS-presented query descriptors that execute through
native bridge read APIs. They are not module-owned query catalog entries yet.

## Validation

Run:

```bash
npm run smoke:storage-query-catalog -w @rusty-crew/brain-island
```

The smoke verifies SQLite and Postgres backend capability differences,
module-derived simple_kv query metadata, storage query execution, and the
fail-closed unmapped Rust query case.
