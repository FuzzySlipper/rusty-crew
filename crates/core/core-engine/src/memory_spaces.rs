use rusty_crew_core_persistence::ProfileMemoryCaps;
use rusty_crew_core_protocol::{
    MemoryConflictPolicy, MemoryDiagnosticsPolicy, MemoryEvidenceKind, MemoryExportImportPolicy,
    MemoryFieldType, MemoryGovernanceMode, MemoryIndexingPolicy, MemoryOperation,
    MemoryOperationPolicy, MemoryPromptPolicy, MemoryProvenancePolicy, MemoryRecordFieldDescriptor,
    MemoryRecordShapeDescriptor, MemoryRecordShapeId, MemoryRetentionPolicy,
    MemoryRetrievalStrategy, MemoryScopeModel, MemoryScopeType, MemorySpaceDescriptor,
    MemorySpaceId, MemoryVisibilityModel, MemoryWritePolicy,
};

pub(crate) fn profile_dense_descriptor(caps: &ProfileMemoryCaps) -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("profile_dense"),
        schema_version: 1,
        module_id: Some("runtime_memory".to_string()),
        description:
            "Compact stable Rusty Crew profile/user memory backed by existing profile_memories."
                .to_string(),
        record_shapes: vec![MemoryRecordShapeDescriptor {
            shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
            version: 1,
            description:
                "Keyed profile or user memory record with revision-token conflict control."
                    .to_string(),
            fields: vec![
                field("profile_id", MemoryFieldType::String, true),
                field("target_type", MemoryFieldType::String, true),
                field("target_id", MemoryFieldType::String, true),
                field("key", MemoryFieldType::String, true),
                field("content", MemoryFieldType::Markdown, true),
                field("metadata_json", MemoryFieldType::Json, false),
                field("revision", MemoryFieldType::Integer, true),
                field("created_at", MemoryFieldType::Timestamp, true),
                field("updated_at", MemoryFieldType::Timestamp, true),
            ],
        }],
        scope_model: MemoryScopeModel {
            allowed_scopes: vec![MemoryScopeType::Profile, MemoryScopeType::User],
            primary_scope: MemoryScopeType::Profile,
        },
        visibility_model: MemoryVisibilityModel::ProfileLocal,
        retrieval_strategies: vec![
            MemoryRetrievalStrategy::DirectLookup,
            MemoryRetrievalStrategy::QuerySearch,
        ],
        indexing: MemoryIndexingPolicy {
            required_capabilities: vec![
                "profile_target_key_lookup".to_string(),
                "expected_revision_conflicts".to_string(),
            ],
            optional_capabilities: vec![
                "profile_scoped_listing".to_string(),
                format!(
                    "cap_max_records_per_profile_{}",
                    caps.max_records_per_profile
                ),
                format!("cap_max_key_bytes_{}", caps.max_key_bytes),
                format!("cap_max_content_bytes_{}", caps.max_content_bytes),
            ],
        },
        prompt_policy: MemoryPromptPolicy::SummaryContext,
        write_policy: MemoryWritePolicy {
            default_mode: MemoryGovernanceMode::DirectWrite,
            operation_policies: vec![
                operation_policy(MemoryOperation::Add, false),
                operation_policy(MemoryOperation::Replace, true),
                operation_policy(MemoryOperation::Remove, true),
            ],
        },
        operations: vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Replace,
            MemoryOperation::Remove,
        ],
        provenance_policy: MemoryProvenancePolicy {
            required_evidence: vec![MemoryEvidenceKind::Wake],
            source_required: false,
            rationale_required: false,
        },
        retention_policy: MemoryRetentionPolicy::ManualOnly,
        conflict_policy: MemoryConflictPolicy::ExpectedRevision,
        diagnostics: MemoryDiagnosticsPolicy {
            expose_catalog: true,
            expose_record_counts: true,
            expose_policy_decisions: true,
        },
        export_import: MemoryExportImportPolicy {
            export_supported: true,
            import_supported: true,
            import_governance_mode: MemoryGovernanceMode::ManualReview,
        },
    }
}

fn field(
    field_name: &str,
    field_type: MemoryFieldType,
    required: bool,
) -> MemoryRecordFieldDescriptor {
    MemoryRecordFieldDescriptor {
        field_name: field_name.to_string(),
        field_type,
        required,
        description: format!("{field_name} field"),
    }
}

fn operation_policy(
    operation: MemoryOperation,
    requires_expected_revision: bool,
) -> MemoryOperationPolicy {
    MemoryOperationPolicy {
        operation,
        governance_mode: MemoryGovernanceMode::DirectWrite,
        requires_expected_revision,
        min_confidence: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_dense_descriptor_matches_existing_dense_memory_contract() {
        let descriptor = profile_dense_descriptor(&ProfileMemoryCaps::default());
        descriptor.validate().expect("descriptor is valid");
        assert_eq!(descriptor.space_id.as_str(), "profile_dense");
        assert_eq!(
            descriptor.scope_model.allowed_scopes,
            vec![MemoryScopeType::Profile, MemoryScopeType::User]
        );
        assert_eq!(
            descriptor.operations,
            vec![
                MemoryOperation::Read,
                MemoryOperation::List,
                MemoryOperation::Add,
                MemoryOperation::Replace,
                MemoryOperation::Remove,
            ]
        );
        assert!(descriptor
            .write_policy
            .operation_policies
            .iter()
            .any(|policy| policy.operation == MemoryOperation::Replace
                && policy.requires_expected_revision));
        assert_eq!(
            descriptor.conflict_policy,
            MemoryConflictPolicy::ExpectedRevision
        );
        assert_eq!(descriptor.prompt_policy, MemoryPromptPolicy::SummaryContext);
    }
}
