# Future pi-crew And Hermes Migration Metadata

Status: Implementation contract for task 2878

Date: 2026-06-20

## Purpose

Rusty Crew should be able to host future pi-crew and Hermes agents without
forcing their legacy storage models into the near-term runtime schema. The
runtime tables should keep describing Rusty Crew state. Migration metadata lives
beside them as provenance and legacy ID mapping.

No pi-crew or Hermes migration is implemented yet. This is only the attachment
point for future import tools.

## Existing Identity Fields

Durable agent, instance, and session identity records already carry:

- source system;
- source external ID;
- Den project/task reference;
- created/active/archive timestamps.

Those fields are for the runtime object itself. They should be used when an
imported agent, instance, or session has a clear Rusty Crew equivalent.

## Import Batches

Schema version 10 adds `runtime_import_batches`.

An import batch identifies a source snapshot, such as:

- a pi-crew production database backup;
- a pi-crew worker-run history export;
- a Hermes profile SQLite directory;
- a single Hermes profile database.

The batch stores source system, label, optional snapshot reference, notes, and
import timestamp. It does not imply that all source data was copied into Rusty
Crew.

## Legacy ID Mappings

Schema version 10 adds `legacy_id_mappings`.

Each mapping records:

- import batch ID;
- source system;
- legacy object kind and legacy ID;
- Rusty Crew object kind and Rusty Crew ID;
- structured provenance JSON;
- mapping creation timestamp.

The mapping key is `(source_system, legacy_kind, legacy_id)`, so re-importing a
known legacy object updates its current Rusty Crew target rather than creating
ambiguous duplicate mappings.

## Provenance Shape

`RuntimeImportProvenance` can attach:

- Rusty Crew profile ID;
- Rusty Crew session ID;
- Rusty Crew agent ID;
- whether the source object remains externally owned;
- short notes.

External ownership is important for Hermes. A Hermes profile SQLite database can
be referenced as an external artifact without copying every record into Rusty
Crew tables.

## pi-crew Compatibility Notes

pi-crew history is strongly shaped by worker pools and orchestrator-managed
child runs. Rusty Crew should map only durable facts into its runtime model:

- durable agents, instances, and sessions when they have active Rusty Crew
  equivalents;
- worker-run IDs into worker/delegated run mappings;
- completion packet summaries when they are useful as history;
- Den project/task references as Den metadata, not copied Den product data.

Worker-pool implementation details should remain provenance unless Rusty Crew
has a matching runtime object. The import should not recreate pi-crew's older
assumption that worker pools are the primary agent model.

## Hermes Compatibility Notes

Hermes uses one SQLite database per profile. That can remain a source isolation
advantage instead of becoming a schema constraint.

Recommended mapping:

- each Hermes profile database gets an import batch or external-artifact
  mapping;
- profile IDs map to Rusty Crew profile IDs;
- active agents/sessions map only if they are revived in Rusty Crew;
- historical messages can stay external until a search/import tool has a clear
  retention policy.

## Import, Reference, Or Drop

Import into Rusty Crew runtime tables:

- active agents and sessions that Rusty Crew will operate;
- current profile/session configuration needed for execution;
- completion summaries needed for runtime continuity.

Reference externally through import metadata:

- pi-crew historical worker-pool details that do not affect current operation;
- Hermes per-profile database paths;
- large historical message archives;
- old diagnostics or debug logs.

Intentionally drop:

- transient queue items that are expired or unsafe to replay;
- stale wake requests;
- local process handles and worker lease records;
- cached model/provider lookups that can be rebuilt;
- historical implementation-specific counters that cannot be reconciled with
  Rusty Crew counter semantics.

## Boundary Rule

Migration tools should call typed persistence APIs:

- `save_import_batch`;
- `load_import_batches`;
- `save_legacy_id_mapping`;
- `query_legacy_id_mappings`;
- existing identity/session/config APIs.

They should not add pi-crew or Hermes assumptions to engine code. If a migration
needs a new runtime concept, create that concept deliberately rather than hiding
it in provenance.
