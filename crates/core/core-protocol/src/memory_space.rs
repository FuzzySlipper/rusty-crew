use crate::{CoreError, CoreErrorKind, CoreResult, IsoTimestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

const MAX_IDENTIFIER_LEN: usize = 64;
const MAX_SCOPE_ID_LEN: usize = 256;

/// Runtime-owned durable memory space id.
///
/// Crew memory spaces live in Rusty Crew service storage and may be projected
/// into prompts according to their descriptor policy. They are distinct from
/// Den memory, which is external Den-owned product memory and should only enter
/// Crew memory through explicit import/proposal flows with provenance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemorySpaceId(pub String);

impl MemorySpaceId {
    pub fn new(raw: impl Into<String>) -> CoreResult<Self> {
        let raw = raw.into();
        validate_identifier("memory space id", &raw)?;
        Ok(Self(raw))
    }

    pub fn unchecked(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> CoreResult<()> {
        validate_identifier("memory space id", &self.0)
    }
}

impl fmt::Display for MemorySpaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Record shape id inside a memory space, such as `profile_dense_item`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemoryRecordShapeId(pub String);

impl MemoryRecordShapeId {
    pub fn new(raw: impl Into<String>) -> CoreResult<Self> {
        let raw = raw.into();
        validate_identifier("memory record shape id", &raw)?;
        Ok(Self(raw))
    }

    pub fn unchecked(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> CoreResult<()> {
        validate_identifier("memory record shape id", &self.0)
    }
}

impl fmt::Display for MemoryRecordShapeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryScopeType {
    Profile,
    User,
    Session,
    ConversationBranch,
    World,
    Entity,
    Project,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryScope {
    pub scope_type: MemoryScopeType,
    pub scope_id: String,
}

impl MemoryScope {
    pub fn validate(&self) -> CoreResult<()> {
        validate_scope_id(&self.scope_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRetrievalStrategy {
    DirectLookup,
    QuerySearch,
    Recency,
    Relevance,
    BranchAware,
    DomainSpecific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPromptPolicy {
    AutoContext,
    SummaryContext,
    ToolOnly,
    ExplicitUserContext,
    NeverPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryGovernanceMode {
    ReadOnly,
    DirectWrite,
    Candidate,
    ManualReview,
    CuratorRoute,
    AutoApplyThreshold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryOperation {
    Read,
    List,
    Add,
    Replace,
    Merge,
    Supersede,
    Remove,
    Archive,
    CandidateOnly,
}

impl MemoryOperation {
    pub fn is_proposal_operation(self) -> bool {
        !matches!(self, Self::Read | Self::List)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryVisibilityModel {
    ProfileLocal,
    UserScoped,
    SessionScoped,
    WorldScoped,
    ProjectScoped,
    ServiceInternal,
    ExplicitPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryFieldType {
    String,
    Markdown,
    Json,
    Integer,
    Float,
    Boolean,
    Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEvidenceKind {
    Wake,
    Event,
    ToolCall,
    Transcript,
    UserCorrection,
    SourceDocument,
    /// Explicit bridge/import evidence from external Den-owned memory.
    DenMemory,
    Import,
    Migration,
    Ui,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRetentionPolicy {
    ManualOnly,
    Expire,
    Archive,
    Tombstone,
    Compact,
    DomainSpecific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryConflictPolicy {
    ExpectedRevision,
    Supersession,
    Merge,
    Immutable,
    DomainSpecific,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryProposalSource {
    InWakeTool,
    CaptureProducer,
    Ui,
    Import,
    Migration,
    Human,
    DenMemoryImport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordFieldDescriptor {
    pub field_name: String,
    pub field_type: MemoryFieldType,
    pub required: bool,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordShapeDescriptor {
    pub shape_id: MemoryRecordShapeId,
    pub version: u32,
    pub description: String,
    pub fields: Vec<MemoryRecordFieldDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryScopeModel {
    pub allowed_scopes: Vec<MemoryScopeType>,
    pub primary_scope: MemoryScopeType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryIndexingPolicy {
    pub required_capabilities: Vec<String>,
    pub optional_capabilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryOperationPolicy {
    pub operation: MemoryOperation,
    pub governance_mode: MemoryGovernanceMode,
    pub requires_expected_revision: bool,
    pub min_confidence: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryWritePolicy {
    pub default_mode: MemoryGovernanceMode,
    pub operation_policies: Vec<MemoryOperationPolicy>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProvenancePolicy {
    pub required_evidence: Vec<MemoryEvidenceKind>,
    pub source_required: bool,
    pub rationale_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryDiagnosticsPolicy {
    pub expose_catalog: bool,
    pub expose_record_counts: bool,
    pub expose_policy_decisions: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryExportImportPolicy {
    pub export_supported: bool,
    pub import_supported: bool,
    pub import_governance_mode: MemoryGovernanceMode,
}

/// Rust-owned memory-space descriptor projected outward to TypeScript clients.
///
/// TypeScript may consume these descriptors for UI, tools, adapters, and
/// proposal validation, but physical registration and storage ownership remain
/// in Rusty Crew core. Descriptors describe Crew runtime memory; external Den
/// memory remains a separate Den service unless explicitly imported/proposed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemorySpaceDescriptor {
    pub space_id: MemorySpaceId,
    pub schema_version: u32,
    pub module_id: Option<String>,
    pub description: String,
    pub record_shapes: Vec<MemoryRecordShapeDescriptor>,
    pub scope_model: MemoryScopeModel,
    pub visibility_model: MemoryVisibilityModel,
    pub retrieval_strategies: Vec<MemoryRetrievalStrategy>,
    pub indexing: MemoryIndexingPolicy,
    pub prompt_policy: MemoryPromptPolicy,
    pub write_policy: MemoryWritePolicy,
    pub operations: Vec<MemoryOperation>,
    pub provenance_policy: MemoryProvenancePolicy,
    pub retention_policy: MemoryRetentionPolicy,
    pub conflict_policy: MemoryConflictPolicy,
    pub diagnostics: MemoryDiagnosticsPolicy,
    pub export_import: MemoryExportImportPolicy,
}

impl MemorySpaceDescriptor {
    pub fn validate(&self) -> CoreResult<()> {
        self.space_id.validate()?;
        if self.schema_version == 0 {
            return invalid("memory space schema_version must be greater than zero");
        }
        if let Some(module_id) = &self.module_id {
            validate_identifier("memory module id", module_id)?;
        }
        if self.record_shapes.is_empty() {
            return invalid("memory space must declare at least one record shape");
        }
        for shape in &self.record_shapes {
            shape.shape_id.validate()?;
            if shape.version == 0 {
                return invalid(format!(
                    "memory record shape {} version must be greater than zero",
                    shape.shape_id
                ));
            }
            if shape.fields.is_empty() {
                return invalid(format!(
                    "memory record shape {} must declare at least one field",
                    shape.shape_id
                ));
            }
            for field in &shape.fields {
                validate_identifier("memory record field name", &field.field_name)?;
            }
        }
        if self.scope_model.allowed_scopes.is_empty() {
            return invalid("memory space must allow at least one scope type");
        }
        if !self
            .scope_model
            .allowed_scopes
            .contains(&self.scope_model.primary_scope)
        {
            return invalid("memory space primary_scope must be in allowed_scopes");
        }
        if self.retrieval_strategies.is_empty() {
            return invalid("memory space must declare at least one retrieval strategy");
        }
        if self.operations.is_empty() {
            return invalid("memory space must declare at least one operation");
        }
        for policy in &self.write_policy.operation_policies {
            if !self.operations.contains(&policy.operation) {
                return invalid(format!(
                    "memory operation policy references unsupported operation {:?}",
                    policy.operation
                ));
            }
            if let Some(min_confidence) = policy.min_confidence {
                validate_confidence(min_confidence)?;
            }
        }
        Ok(())
    }

    pub fn supports_scope(&self, scope_type: MemoryScopeType) -> bool {
        self.scope_model.allowed_scopes.contains(&scope_type)
    }

    pub fn supports_operation(&self, operation: MemoryOperation) -> bool {
        self.operations.contains(&operation)
    }

    pub fn has_shape(&self, shape: &MemoryRecordShapeRef) -> bool {
        self.record_shapes.iter().any(|candidate| {
            candidate.shape_id == shape.shape_id && candidate.version == shape.version
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecordShapeRef {
    pub shape_id: MemoryRecordShapeId,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryEvidenceRef {
    pub evidence_type: MemoryEvidenceKind,
    pub ref_id: String,
    pub label: Option<String>,
}

/// Proposed Crew memory mutation.
///
/// This envelope may be produced by a brain/tool/UI/importer. It is not a write
/// by itself: Rust validates it against the Rust-owned descriptor and routes it
/// through the descriptor's governance policy before any Crew storage changes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryProposalEnvelope {
    pub proposal_id: String,
    pub space_id: MemorySpaceId,
    pub operation: MemoryOperation,
    pub scope: MemoryScope,
    pub shape: MemoryRecordShapeRef,
    pub content: Value,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub confidence: f32,
    pub durability_rationale: Option<String>,
    pub governance_mode: MemoryGovernanceMode,
    pub source: MemoryProposalSource,
    pub dedupe_key: Option<String>,
    pub created_at: Option<IsoTimestamp>,
}

impl MemoryProposalEnvelope {
    pub fn validate_for_descriptor(&self, descriptor: &MemorySpaceDescriptor) -> CoreResult<()> {
        descriptor.validate()?;
        validate_identifier("memory proposal id", &self.proposal_id)?;
        if self.space_id != descriptor.space_id {
            return invalid("memory proposal space_id does not match descriptor");
        }
        if !self.operation.is_proposal_operation() {
            return invalid("memory proposal operation must mutate memory");
        }
        if !descriptor.supports_operation(self.operation) {
            return invalid("memory proposal operation is not supported by descriptor");
        }
        self.scope.validate()?;
        if !descriptor.supports_scope(self.scope.scope_type) {
            return invalid("memory proposal scope_type is not supported by descriptor");
        }
        self.shape.shape_id.validate()?;
        if self.shape.version == 0 {
            return invalid("memory proposal shape version must be greater than zero");
        }
        if !descriptor.has_shape(&self.shape) {
            return invalid("memory proposal shape is not declared by descriptor");
        }
        validate_confidence(self.confidence)?;
        for evidence in &self.evidence_refs {
            if evidence.ref_id.trim().is_empty() {
                return invalid("memory proposal evidence ref_id must not be empty");
            }
        }
        for required in &descriptor.provenance_policy.required_evidence {
            if !self
                .evidence_refs
                .iter()
                .any(|evidence| evidence.evidence_type == *required)
            {
                return invalid(format!(
                    "memory proposal missing required evidence {:?}",
                    required
                ));
            }
        }
        if descriptor.provenance_policy.rationale_required
            && self
                .durability_rationale
                .as_ref()
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
        {
            return invalid("memory proposal durability_rationale is required");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryProposalReviewStatus {
    PendingReview,
    Approved,
    Rejected,
    Applied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryProposalRecord {
    pub proposal: MemoryProposalEnvelope,
    pub status: MemoryProposalReviewStatus,
    pub selected_governance_mode: MemoryGovernanceMode,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
    pub decided_at: Option<IsoTimestamp>,
    pub applied_at: Option<IsoTimestamp>,
    pub resulting_revision: Option<u64>,
    pub duplicate_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryProposalQuery {
    pub space_id: Option<MemorySpaceId>,
    pub status: Option<MemoryProposalReviewStatus>,
    pub dedupe_key: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryGovernanceDecisionKind {
    RoutedToReview,
    Approved,
    Rejected,
    Applied,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryGovernanceDecisionInput {
    pub decision_id: String,
    pub proposal_id: String,
    pub decision: MemoryGovernanceDecisionKind,
    pub actor: String,
    pub source: MemoryProposalSource,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub policy_mode: MemoryGovernanceMode,
    pub confidence: Option<f32>,
    pub message: Option<String>,
    pub resulting_revision: Option<u64>,
    pub decided_at: Option<IsoTimestamp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryGovernanceDecisionRecord {
    pub decision_id: String,
    pub proposal_id: String,
    pub decision: MemoryGovernanceDecisionKind,
    pub actor: String,
    pub source: MemoryProposalSource,
    pub evidence_refs: Vec<MemoryEvidenceRef>,
    pub policy_mode: MemoryGovernanceMode,
    pub confidence: Option<f32>,
    pub message: Option<String>,
    pub resulting_revision: Option<u64>,
    pub decided_at: IsoTimestamp,
}

fn validate_identifier(label: &str, value: &str) -> CoreResult<()> {
    if value.is_empty() {
        return invalid(format!("{label} must not be empty"));
    }
    if value.len() > MAX_IDENTIFIER_LEN {
        return invalid(format!(
            "{label} must be at most {MAX_IDENTIFIER_LEN} characters"
        ));
    }
    let mut previous_underscore = false;
    for (index, ch) in value.chars().enumerate() {
        let valid = ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_';
        if !valid {
            return invalid(format!(
                "{label} must use lowercase snake_case ASCII identifiers"
            ));
        }
        if index == 0 && (!ch.is_ascii_lowercase()) {
            return invalid(format!("{label} must start with a lowercase letter"));
        }
        if ch == '_' && (index == 0 || previous_underscore) {
            return invalid(format!(
                "{label} must not contain leading or repeated underscores"
            ));
        }
        previous_underscore = ch == '_';
    }
    if value.ends_with('_') {
        return invalid(format!("{label} must not end with an underscore"));
    }
    Ok(())
}

fn validate_scope_id(value: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return invalid("memory scope_id must not be empty");
    }
    if value.len() > MAX_SCOPE_ID_LEN {
        return invalid(format!(
            "memory scope_id must be at most {MAX_SCOPE_ID_LEN} characters"
        ));
    }
    if value.contains('\0') {
        return invalid("memory scope_id must not contain NUL");
    }
    Ok(())
}

fn validate_confidence(value: f32) -> CoreResult<()> {
    if !(0.0..=1.0).contains(&value) || value.is_nan() {
        return invalid("memory confidence must be between 0 and 1");
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> CoreResult<T> {
    Err(CoreError::new(CoreErrorKind::InvalidInput, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn valid_examples_cover_adr_memory_spaces() {
        profile_dense_descriptor()
            .validate()
            .expect("profile dense valid");
        session_memory_descriptor()
            .validate()
            .expect("session memory valid");
        roleplay_lore_descriptor()
            .validate()
            .expect("roleplay lore valid");
    }

    #[test]
    fn rejects_invalid_ids_scopes_and_operation_policies() {
        assert!(MemorySpaceId::new("ProfileDense").is_err());

        let mut descriptor = profile_dense_descriptor();
        descriptor.scope_model.primary_scope = MemoryScopeType::World;
        assert!(descriptor.validate().is_err());

        let mut descriptor = profile_dense_descriptor();
        descriptor
            .write_policy
            .operation_policies
            .push(MemoryOperationPolicy {
                operation: MemoryOperation::Archive,
                governance_mode: MemoryGovernanceMode::ManualReview,
                requires_expected_revision: false,
                min_confidence: None,
            });
        assert!(descriptor.validate().is_err());
    }

    #[test]
    fn validates_proposals_against_descriptor_policy() {
        let descriptor = profile_dense_descriptor();
        let proposal = valid_profile_dense_proposal(MemoryOperation::Add);
        proposal
            .validate_for_descriptor(&descriptor)
            .expect("proposal matches descriptor");

        let mut wrong_scope = proposal.clone();
        wrong_scope.scope.scope_type = MemoryScopeType::World;
        assert!(wrong_scope.validate_for_descriptor(&descriptor).is_err());

        let mut read_operation = proposal.clone();
        read_operation.operation = MemoryOperation::Read;
        assert!(read_operation.validate_for_descriptor(&descriptor).is_err());

        let mut missing_evidence = proposal.clone();
        missing_evidence.evidence_refs.clear();
        assert!(missing_evidence
            .validate_for_descriptor(&descriptor)
            .is_err());

        let mut bad_confidence = proposal;
        bad_confidence.confidence = 1.25;
        assert!(bad_confidence.validate_for_descriptor(&descriptor).is_err());
    }

    #[test]
    fn serializes_descriptor_and_proposal_with_snake_case_enums() {
        let descriptor = roleplay_lore_descriptor();
        let value = serde_json::to_value(&descriptor).expect("serialize descriptor");
        assert_eq!(value["space_id"], "roleplay_lore");
        assert_eq!(value["scope_model"]["allowed_scopes"][0], "world");
        assert_eq!(value["prompt_policy"], "explicit_user_context");

        let round_trip: MemorySpaceDescriptor =
            serde_json::from_value(value).expect("deserialize descriptor");
        assert_eq!(round_trip, descriptor);

        let proposal = valid_profile_dense_proposal(MemoryOperation::Replace);
        let value = serde_json::to_value(&proposal).expect("serialize proposal");
        assert_eq!(value["operation"], "replace");
        assert_eq!(value["evidence_refs"][0]["evidence_type"], "wake");
        let round_trip: MemoryProposalEnvelope =
            serde_json::from_value(value).expect("deserialize proposal");
        assert_eq!(round_trip, proposal);
    }

    fn profile_dense_descriptor() -> MemorySpaceDescriptor {
        MemorySpaceDescriptor {
            space_id: MemorySpaceId::unchecked("profile_dense"),
            schema_version: 1,
            module_id: Some("runtime_memory".to_string()),
            description: "Compact stable Crew profile memory; not Den memory.".to_string(),
            record_shapes: vec![MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
                description: "Keyed profile or user memory item.".to_string(),
                fields: vec![
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
                required_capabilities: vec!["key_lookup".to_string()],
                optional_capabilities: vec!["text_search".to_string()],
            },
            prompt_policy: MemoryPromptPolicy::SummaryContext,
            write_policy: MemoryWritePolicy {
                default_mode: MemoryGovernanceMode::Candidate,
                operation_policies: vec![
                    op_policy(MemoryOperation::Add, MemoryGovernanceMode::Candidate, false),
                    op_policy(
                        MemoryOperation::Replace,
                        MemoryGovernanceMode::Candidate,
                        true,
                    ),
                    op_policy(
                        MemoryOperation::Remove,
                        MemoryGovernanceMode::Candidate,
                        true,
                    ),
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
                source_required: true,
                rationale_required: true,
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

    fn session_memory_descriptor() -> MemorySpaceDescriptor {
        let mut descriptor = profile_dense_descriptor();
        descriptor.space_id = MemorySpaceId::unchecked("session_memory");
        descriptor.module_id = Some("runtime_memory".to_string());
        descriptor.description = "Crew session and branch memory summaries.".to_string();
        descriptor.record_shapes = vec![
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                version: 1,
                description: "Fact observed inside a session.".to_string(),
                fields: vec![
                    field("content", MemoryFieldType::Markdown, true),
                    field("confidence", MemoryFieldType::Float, false),
                ],
            },
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("branch_summary"),
                version: 1,
                description: "Conversation branch summary.".to_string(),
                fields: vec![field("summary", MemoryFieldType::Markdown, true)],
            },
        ];
        descriptor.scope_model = MemoryScopeModel {
            allowed_scopes: vec![
                MemoryScopeType::Session,
                MemoryScopeType::ConversationBranch,
            ],
            primary_scope: MemoryScopeType::Session,
        };
        descriptor.visibility_model = MemoryVisibilityModel::SessionScoped;
        descriptor.retrieval_strategies = vec![
            MemoryRetrievalStrategy::Recency,
            MemoryRetrievalStrategy::BranchAware,
        ];
        descriptor.prompt_policy = MemoryPromptPolicy::SummaryContext;
        descriptor.operations = vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Merge,
            MemoryOperation::Supersede,
        ];
        descriptor.write_policy.operation_policies = vec![
            op_policy(MemoryOperation::Add, MemoryGovernanceMode::Candidate, false),
            op_policy(
                MemoryOperation::Merge,
                MemoryGovernanceMode::CuratorRoute,
                true,
            ),
            op_policy(
                MemoryOperation::Supersede,
                MemoryGovernanceMode::CuratorRoute,
                true,
            ),
        ];
        descriptor.conflict_policy = MemoryConflictPolicy::Supersession;
        descriptor
    }

    fn roleplay_lore_descriptor() -> MemorySpaceDescriptor {
        let mut descriptor = session_memory_descriptor();
        descriptor.space_id = MemorySpaceId::unchecked("roleplay_lore");
        descriptor.module_id = Some("roleplay_lore".to_string());
        descriptor.description =
            "Crew-owned roleplay lore with canon-aware governance.".to_string();
        descriptor.record_shapes = vec![
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("lore_entry"),
                version: 1,
                description: "World or entity lore entry.".to_string(),
                fields: vec![
                    field("title", MemoryFieldType::String, true),
                    field("body", MemoryFieldType::Markdown, true),
                    field("canon_status", MemoryFieldType::String, true),
                ],
            },
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("timeline_event"),
                version: 1,
                description: "Canon timeline event.".to_string(),
                fields: vec![
                    field("event_time", MemoryFieldType::String, false),
                    field("body", MemoryFieldType::Markdown, true),
                ],
            },
        ];
        descriptor.scope_model = MemoryScopeModel {
            allowed_scopes: vec![
                MemoryScopeType::World,
                MemoryScopeType::Entity,
                MemoryScopeType::Session,
                MemoryScopeType::ConversationBranch,
            ],
            primary_scope: MemoryScopeType::World,
        };
        descriptor.visibility_model = MemoryVisibilityModel::WorldScoped;
        descriptor.retrieval_strategies = vec![
            MemoryRetrievalStrategy::QuerySearch,
            MemoryRetrievalStrategy::Relevance,
            MemoryRetrievalStrategy::DomainSpecific,
        ];
        descriptor.prompt_policy = MemoryPromptPolicy::ExplicitUserContext;
        descriptor.write_policy.default_mode = MemoryGovernanceMode::ManualReview;
        descriptor.retention_policy = MemoryRetentionPolicy::DomainSpecific;
        descriptor.conflict_policy = MemoryConflictPolicy::DomainSpecific;
        descriptor
    }

    fn valid_profile_dense_proposal(operation: MemoryOperation) -> MemoryProposalEnvelope {
        MemoryProposalEnvelope {
            proposal_id: "proposal_one".to_string(),
            space_id: MemorySpaceId::unchecked("profile_dense"),
            operation,
            scope: MemoryScope {
                scope_type: MemoryScopeType::Profile,
                scope_id: "rusty-crew-runner".to_string(),
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
                version: 1,
            },
            content: json!({
                "key": "memory_boundary",
                "content": "Use Crew profile memory for stable local preferences."
            }),
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-1".to_string(),
                label: Some("LLM wake".to_string()),
            }],
            confidence: 0.82,
            durability_rationale: Some("Stable user preference.".to_string()),
            governance_mode: MemoryGovernanceMode::Candidate,
            source: MemoryProposalSource::InWakeTool,
            dedupe_key: Some("profile_dense:memory_boundary".to_string()),
            created_at: Some("2026-06-26T00:00:00Z".to_string()),
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

    fn op_policy(
        operation: MemoryOperation,
        governance_mode: MemoryGovernanceMode,
        requires_expected_revision: bool,
    ) -> MemoryOperationPolicy {
        MemoryOperationPolicy {
            operation,
            governance_mode,
            requires_expected_revision,
            min_confidence: Some(0.5),
        }
    }
}
