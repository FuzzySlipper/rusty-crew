//! Internal repository boundary catalog for Rust-owned coordination storage.
//!
//! Public callers still use `CoordinationStore` methods. This catalog keeps the
//! backend requirements for each runtime concern visible while the monolithic
//! implementation is split into repository modules behind that API.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRepositoryBackendRequirement {
    pub capability: String,
    pub required: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRepositoryGroupDiagnostic {
    pub group_id: String,
    pub label: String,
    pub correctness_sensitive: bool,
    pub backend_requirements: Vec<RuntimeRepositoryBackendRequirement>,
    pub notes: Vec<String>,
}

struct RepositoryGroupDescriptor {
    group_id: &'static str,
    label: &'static str,
    correctness_sensitive: bool,
    backend_requirements: &'static [RepositoryBackendRequirementDescriptor],
    notes: &'static [&'static str],
}

struct RepositoryBackendRequirementDescriptor {
    capability: &'static str,
    required: bool,
    detail: &'static str,
}

const TXN: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "transactions",
    required: true,
    detail: "repository writes must be atomic across their owned rows",
};

const JSON: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "json_metadata",
    required: true,
    detail: "typed metadata and payload envelopes are persisted as validated JSON text",
};

const FTS: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "runtime_full_text_search",
    required: true,
    detail: "runtime search requires backend-supported indexing or an equivalent search table",
};

const PLAN: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "query_plan_diagnostics",
    required: false,
    detail: "diagnostic routes should expose hot query plan checks when the backend supports them",
};

const SIZE: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "estimated_table_size",
    required: false,
    detail: "admin diagnostics can use approximate or exact table size/count projections",
};

const ROW_CLAIMS: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "row_level_claims",
    required: true,
    detail: "multi-writer backends must provide safe job/message claim semantics",
};

const ADVISORY_LOCKS: RepositoryBackendRequirementDescriptor =
    RepositoryBackendRequirementDescriptor {
        capability: "advisory_locks",
        required: false,
        detail:
            "cross-process maintenance and import/export guards can use advisory locks when present",
    };

const LISTEN_NOTIFY: RepositoryBackendRequirementDescriptor =
    RepositoryBackendRequirementDescriptor {
        capability: "listen_notify",
        required: false,
        detail: "multi-service wake fanout can use backend notifications when present",
    };

const LOGICAL_EXPORT: RepositoryBackendRequirementDescriptor =
    RepositoryBackendRequirementDescriptor {
        capability: "logical_export_import",
        required: false,
        detail: "cross-backend moves require a logical transfer path instead of raw file copies",
    };

const ONLINE_MIGRATIONS: RepositoryBackendRequirementDescriptor = RepositoryBackendRequirementDescriptor {
    capability: "online_migrations",
    required: false,
    detail: "larger deployments benefit from migrations that do not require service startup exclusivity",
};

const CORE_REPOSITORY_GROUPS: &[RepositoryGroupDescriptor] = &[
    RepositoryGroupDescriptor {
        group_id: "storage_admin",
        label: "Storage Admin",
        correctness_sensitive: false,
        backend_requirements: &[TXN, SIZE, PLAN, ONLINE_MIGRATIONS],
        notes: &[
            "Owns schema version, migrations, size, maintenance, and backend capability diagnostics.",
            "SQLite remains the active implementation; Postgres support must satisfy this group before becoming selectable.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "sessions_identities",
        label: "Sessions And Identities",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns sessions, durable agents, agent instances, identity links, and immutable session config snapshots.",
            "Session hydration must stay backend-agnostic at the public API boundary.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "events_projections",
        label: "Events And Body Projections",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE, PLAN],
        notes: &[
            "Owns core event logs and body projection records used for restart hydration.",
            "Projection writes must not be split across independent adapter-side stores.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "queues_messages",
        label: "Queues And Messages",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, ROW_CLAIMS, SIZE, PLAN],
        notes: &[
            "Owns queued messages, TTL expiry, terminal purge, and internal agent messages.",
            "Queue claims and expiry are correctness-sensitive because stale messages must not resurrect after restart.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "scheduler_jobs",
        label: "Scheduler Jobs",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, ROW_CLAIMS, ADVISORY_LOCKS, LISTEN_NOTIFY, PLAN],
        notes: &[
            "Owns scheduled jobs, run claims, stale claim expiry, and manual run requests.",
            "Second-backend proof must make multi-process claim behavior explicit.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "worker_runs_completions",
        label: "Worker Runs And Completions",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns worker run state, completion packets, delegation lineage, and fan-out groups.",
            "Completion records are lifecycle evidence and must remain transactional with their coordination updates.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "tool_telemetry",
        label: "Tool Telemetry",
        correctness_sensitive: false,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns tool call metadata and runtime tool counters.",
            "Telemetry loss is degraded observability, not a reason to route around Rust coordination.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "provider_state",
        label: "Provider State",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns provider wire state and expiry for modular brain implementations.",
            "Provider state must be scoped so model/provider changes can rebuild runtime brain state safely.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "runtime_search",
        label: "Runtime Search",
        correctness_sensitive: true,
        backend_requirements: &[TXN, FTS, PLAN],
        notes: &[
            "Owns indexed runtime search over sessions, messages, and queued messages.",
            "Search is a read-model green path for users and agents to inspect what happened.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "conversations_attachments",
        label: "Conversations And Attachments",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE, PLAN],
        notes: &[
            "Owns conversation branches, snapshots, slots, variants, attachments, and data-bank scopes.",
            "Transcript and lore-facing data require stable ordering and branch-head semantics.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "profile_memory",
        label: "Profile Memory",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE, PLAN],
        notes: &[
            "Owns dense profile memory plus memory proposal/governance records.",
            "Future lore/custom memory shapes should use this boundary or module-owned repository contracts.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "bindings",
        label: "Bindings",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns MCP, channel, and adapter binding records needed to restart profiles without file edits.",
            "Bindings must remain profile/session scoped rather than service-global by accident.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "profile_registry",
        label: "Profile Registry",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE],
        notes: &[
            "Owns the registry records produced by the official create-profile path.",
            "Filesystem profile bundles remain input/output material, not the only source of runtime plumbing truth.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "module_schema_registry",
        label: "Module Schema Registry",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, SIZE, LOGICAL_EXPORT],
        notes: &[
            "Owns installed module schema descriptors, repository contracts, and transfer hook declarations.",
            "Module-owned tables must declare backend capabilities before they become durable service state.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "import_export",
        label: "Import And Export",
        correctness_sensitive: true,
        backend_requirements: &[TXN, JSON, LOGICAL_EXPORT, ADVISORY_LOCKS],
        notes: &[
            "Owns import batches, legacy mapping records, and future logical transfer orchestration.",
            "Raw SQLite-to-Postgres migration is intentionally not the green path.",
        ],
    },
    RepositoryGroupDescriptor {
        group_id: "runtime_counters",
        label: "Runtime Counters",
        correctness_sensitive: false,
        backend_requirements: &[TXN, SIZE],
        notes: &[
            "Owns durable runtime counters and summaries.",
            "Counters support diagnostics and should not be used as sole lifecycle truth.",
        ],
    },
];

pub(crate) fn core_repository_group_diagnostics() -> Vec<RuntimeRepositoryGroupDiagnostic> {
    CORE_REPOSITORY_GROUPS
        .iter()
        .map(|group| RuntimeRepositoryGroupDiagnostic {
            group_id: group.group_id.to_string(),
            label: group.label.to_string(),
            correctness_sensitive: group.correctness_sensitive,
            backend_requirements: group
                .backend_requirements
                .iter()
                .map(|requirement| RuntimeRepositoryBackendRequirement {
                    capability: requirement.capability.to_string(),
                    required: requirement.required,
                    detail: requirement.detail.to_string(),
                })
                .collect(),
            notes: group.notes.iter().map(|note| (*note).to_string()).collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_catalog_names_correctness_sensitive_groups() {
        let groups = core_repository_group_diagnostics();
        for group_id in [
            "queues_messages",
            "scheduler_jobs",
            "runtime_search",
            "conversations_attachments",
            "profile_memory",
        ] {
            let group = groups
                .iter()
                .find(|candidate| candidate.group_id == group_id)
                .unwrap_or_else(|| panic!("missing repository group {group_id}"));
            assert!(
                group.correctness_sensitive,
                "{group_id} must be marked correctness-sensitive"
            );
            assert!(
                group
                    .backend_requirements
                    .iter()
                    .any(|requirement| requirement.capability == "transactions"
                        && requirement.required),
                "{group_id} must require transactions"
            );
        }
    }

    #[test]
    fn scheduler_and_queue_groups_document_claim_requirements() {
        let groups = core_repository_group_diagnostics();
        for group_id in ["queues_messages", "scheduler_jobs"] {
            let group = groups
                .iter()
                .find(|candidate| candidate.group_id == group_id)
                .unwrap_or_else(|| panic!("missing repository group {group_id}"));
            assert!(
                group
                    .backend_requirements
                    .iter()
                    .any(|requirement| requirement.capability == "row_level_claims"
                        && requirement.required),
                "{group_id} must require explicit claim semantics for second backends"
            );
        }
    }
}
