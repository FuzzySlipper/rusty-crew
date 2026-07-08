use crate::{CoreError, CoreErrorKind, CoreResult, IsoTimestamp, ProfileId, SessionId};
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

pub fn session_memory_space_descriptor() -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("session_memory"),
        schema_version: 1,
        module_id: Some("runtime_memory".to_string()),
        description:
            "Crew-owned session and branch memory; not Den memory and not transcript storage."
                .to_string(),
        record_shapes: vec![
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                version: 1,
                description: "Durable fact observed inside one session.".to_string(),
                fields: vec![
                    descriptor_field("record_id", MemoryFieldType::String, true),
                    descriptor_field("content", MemoryFieldType::Markdown, true),
                    descriptor_field("fact_kind", MemoryFieldType::String, true),
                    descriptor_field("confidence", MemoryFieldType::Float, true),
                    descriptor_field("source_summary", MemoryFieldType::String, true),
                    descriptor_field("created_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("updated_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("subject", MemoryFieldType::String, false),
                    descriptor_field("expires_at", MemoryFieldType::Timestamp, false),
                    descriptor_field("supersedes_record_id", MemoryFieldType::String, false),
                    descriptor_field("tags", MemoryFieldType::Json, false),
                    descriptor_field("metadata_json", MemoryFieldType::Json, false),
                ],
            },
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("session_summary"),
                version: 1,
                description: "Rolling or checkpoint summary of the durable session.".to_string(),
                fields: vec![
                    descriptor_field("record_id", MemoryFieldType::String, true),
                    descriptor_field("summary", MemoryFieldType::Markdown, true),
                    descriptor_field("coverage_start", MemoryFieldType::String, true),
                    descriptor_field("coverage_end", MemoryFieldType::String, true),
                    descriptor_field("summary_kind", MemoryFieldType::String, true),
                    descriptor_field("created_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("updated_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("token_estimate", MemoryFieldType::Integer, false),
                    descriptor_field("source_record_ids", MemoryFieldType::Json, false),
                    descriptor_field("supersedes_record_id", MemoryFieldType::String, false),
                    descriptor_field("metadata_json", MemoryFieldType::Json, false),
                ],
            },
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("branch_summary"),
                version: 1,
                description: "Conversation branch summary.".to_string(),
                fields: vec![
                    descriptor_field("record_id", MemoryFieldType::String, true),
                    descriptor_field("summary", MemoryFieldType::Markdown, true),
                    descriptor_field("branch_id", MemoryFieldType::String, true),
                    descriptor_field("head_message_id", MemoryFieldType::String, true),
                    descriptor_field("coverage_start", MemoryFieldType::String, true),
                    descriptor_field("coverage_end", MemoryFieldType::String, true),
                    descriptor_field("created_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("updated_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("parent_branch_id", MemoryFieldType::String, false),
                    descriptor_field("ancestor_branch_ids", MemoryFieldType::Json, false),
                    descriptor_field("supersedes_record_id", MemoryFieldType::String, false),
                    descriptor_field("token_estimate", MemoryFieldType::Integer, false),
                    descriptor_field("metadata_json", MemoryFieldType::Json, false),
                ],
            },
            MemoryRecordShapeDescriptor {
                shape_id: MemoryRecordShapeId::unchecked("user_choice"),
                version: 1,
                description: "Durable user choice inside a session or branch.".to_string(),
                fields: vec![
                    descriptor_field("record_id", MemoryFieldType::String, true),
                    descriptor_field("choice", MemoryFieldType::Markdown, true),
                    descriptor_field("choice_kind", MemoryFieldType::String, true),
                    descriptor_field("chosen_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("status", MemoryFieldType::String, true),
                    descriptor_field("created_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("updated_at", MemoryFieldType::Timestamp, true),
                    descriptor_field("alternatives", MemoryFieldType::Json, false),
                    descriptor_field("supersedes_record_id", MemoryFieldType::String, false),
                    descriptor_field("reverted_by_record_id", MemoryFieldType::String, false),
                    descriptor_field("metadata_json", MemoryFieldType::Json, false),
                ],
            },
        ],
        scope_model: MemoryScopeModel {
            allowed_scopes: vec![
                MemoryScopeType::Session,
                MemoryScopeType::ConversationBranch,
            ],
            primary_scope: MemoryScopeType::Session,
        },
        visibility_model: MemoryVisibilityModel::SessionScoped,
        retrieval_strategies: vec![
            MemoryRetrievalStrategy::DirectLookup,
            MemoryRetrievalStrategy::Recency,
            MemoryRetrievalStrategy::BranchAware,
            MemoryRetrievalStrategy::QuerySearch,
        ],
        indexing: MemoryIndexingPolicy {
            required_capabilities: vec!["session_scope_lookup".to_string()],
            optional_capabilities: vec![
                "branch_aware_lookup".to_string(),
                "query_search".to_string(),
            ],
        },
        prompt_policy: MemoryPromptPolicy::SummaryContext,
        write_policy: MemoryWritePolicy {
            default_mode: MemoryGovernanceMode::Candidate,
            operation_policies: vec![
                descriptor_op_policy(MemoryOperation::Add, MemoryGovernanceMode::Candidate, false),
                descriptor_op_policy(
                    MemoryOperation::Replace,
                    MemoryGovernanceMode::CuratorRoute,
                    true,
                ),
                descriptor_op_policy(
                    MemoryOperation::Merge,
                    MemoryGovernanceMode::CuratorRoute,
                    true,
                ),
                descriptor_op_policy(
                    MemoryOperation::Supersede,
                    MemoryGovernanceMode::CuratorRoute,
                    true,
                ),
                descriptor_op_policy(
                    MemoryOperation::Archive,
                    MemoryGovernanceMode::ManualReview,
                    true,
                ),
            ],
        },
        operations: vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Replace,
            MemoryOperation::Merge,
            MemoryOperation::Supersede,
            MemoryOperation::Archive,
        ],
        provenance_policy: MemoryProvenancePolicy {
            required_evidence: vec![MemoryEvidenceKind::Wake],
            source_required: true,
            rationale_required: true,
        },
        retention_policy: MemoryRetentionPolicy::Compact,
        conflict_policy: MemoryConflictPolicy::Supersession,
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

fn descriptor_field(
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

fn descriptor_op_policy(
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPolicyDiagnostic {
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPolicyReport {
    pub accepted: bool,
    pub diagnostics: Vec<MemoryPolicyDiagnostic>,
}

impl MemoryPolicyReport {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            diagnostics: Vec::new(),
        }
    }

    pub fn first_reason_code(&self) -> Option<&str> {
        self.diagnostics
            .first()
            .map(|diagnostic| diagnostic.reason_code.as_str())
    }

    fn reject(reason_code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            diagnostics: vec![MemoryPolicyDiagnostic {
                reason_code: reason_code.into(),
                message: message.into(),
            }],
        }
    }

    fn push_rejection(&mut self, reason_code: impl Into<String>, message: impl Into<String>) {
        self.accepted = false;
        self.diagnostics.push(MemoryPolicyDiagnostic {
            reason_code: reason_code.into(),
            message: message.into(),
        });
    }
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

pub fn select_memory_governance_mode(
    requested: MemoryGovernanceMode,
    source: MemoryProposalSource,
) -> MemoryGovernanceMode {
    match (source, requested) {
        (
            MemoryProposalSource::InWakeTool | MemoryProposalSource::CaptureProducer,
            MemoryGovernanceMode::DirectWrite | MemoryGovernanceMode::AutoApplyThreshold,
        ) => MemoryGovernanceMode::CuratorRoute,
        _ => requested,
    }
}

pub fn evaluate_memory_proposal_policy(
    proposal: &MemoryProposalEnvelope,
    descriptor: &MemorySpaceDescriptor,
) -> MemoryPolicyReport {
    let mut report = MemoryPolicyReport::accepted();
    if let Err(error) = descriptor.validate() {
        report.push_rejection(
            "memory_policy_descriptor_invalid",
            format!("memory descriptor is invalid: {error}"),
        );
        return report;
    }
    if validate_identifier("memory proposal id", &proposal.proposal_id).is_err() {
        report.push_rejection(
            "memory_policy_proposal_id_invalid",
            "memory proposal id must be a lowercase snake_case identifier",
        );
    }
    if proposal.space_id != descriptor.space_id {
        report.push_rejection(
            "memory_policy_space_mismatch",
            "memory proposal space_id does not match descriptor",
        );
    }
    if !proposal.operation.is_proposal_operation() {
        report.push_rejection(
            "memory_policy_read_operation_rejected",
            "memory proposal operation must mutate memory",
        );
    }
    let operation_policy = descriptor
        .write_policy
        .operation_policies
        .iter()
        .find(|policy| policy.operation == proposal.operation);
    if !descriptor.supports_operation(proposal.operation) || operation_policy.is_none() {
        report.push_rejection(
            "memory_policy_operation_unsupported",
            format!(
                "memory operation {:?} is not supported by space {}",
                proposal.operation, descriptor.space_id
            ),
        );
    }
    if let Err(error) = proposal.scope.validate() {
        report.push_rejection(
            "memory_policy_scope_invalid",
            format!("memory proposal scope is invalid: {error}"),
        );
    }
    if !descriptor.supports_scope(proposal.scope.scope_type) {
        report.push_rejection(
            "memory_policy_scope_unsupported",
            format!(
                "memory scope {:?} is not supported by space {}",
                proposal.scope.scope_type, descriptor.space_id
            ),
        );
    }
    if proposal.shape.shape_id.validate().is_err() || proposal.shape.version == 0 {
        report.push_rejection(
            "memory_policy_shape_invalid",
            "memory proposal shape id and version must be valid",
        );
    } else if !descriptor.has_shape(&proposal.shape) {
        report.push_rejection(
            "memory_policy_shape_unsupported",
            format!(
                "memory shape {}@{} is not declared by space {}",
                proposal.shape.shape_id, proposal.shape.version, descriptor.space_id
            ),
        );
    }
    if let Err(error) = validate_confidence(proposal.confidence) {
        report.push_rejection(
            "memory_policy_confidence_invalid",
            format!("memory proposal confidence is invalid: {error}"),
        );
    }
    if let Some(policy) = operation_policy {
        if let Some(min_confidence) = policy.min_confidence {
            if proposal.confidence < min_confidence {
                report.push_rejection(
                    "memory_policy_confidence_below_minimum",
                    format!(
                        "memory proposal confidence {} is below required minimum {}",
                        proposal.confidence, min_confidence
                    ),
                );
            }
        }
        if policy.requires_expected_revision
            && !proposal
                .content
                .get("expected_revision")
                .and_then(Value::as_u64)
                .is_some_and(|revision| revision > 0)
        {
            report.push_rejection(
                "memory_policy_expected_revision_required",
                "memory proposal operation requires content.expected_revision greater than zero",
            );
        }
    }
    for evidence in &proposal.evidence_refs {
        if evidence.ref_id.trim().is_empty() {
            report.push_rejection(
                "memory_policy_evidence_ref_invalid",
                "memory proposal evidence ref_id must not be empty",
            );
        }
    }
    for required in &descriptor.provenance_policy.required_evidence {
        if !proposal
            .evidence_refs
            .iter()
            .any(|evidence| evidence.evidence_type == *required)
        {
            report.push_rejection(
                "memory_policy_required_evidence_missing",
                format!("memory proposal missing required evidence {required:?}"),
            );
        }
    }
    if descriptor.provenance_policy.source_required
        && proposal.source == MemoryProposalSource::DenMemoryImport
        && !proposal
            .evidence_refs
            .iter()
            .any(|evidence| evidence.evidence_type == MemoryEvidenceKind::DenMemory)
    {
        report.push_rejection(
            "memory_policy_den_import_evidence_required",
            "Den memory imports must cite Den memory as external evidence",
        );
    }
    if descriptor.provenance_policy.rationale_required
        && proposal
            .durability_rationale
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        report.push_rejection(
            "memory_policy_rationale_required",
            "memory proposal durability_rationale is required",
        );
    }
    if matches!(descriptor.retention_policy, MemoryRetentionPolicy::Expire)
        && proposal.content.get("expires_at").is_none()
    {
        report.push_rejection(
            "memory_policy_retention_metadata_required",
            "expiring memory proposals must include content.expires_at",
        );
    }
    report
}

pub fn validate_memory_proposal_policy(
    proposal: &MemoryProposalEnvelope,
    descriptor: &MemorySpaceDescriptor,
) -> CoreResult<MemoryPolicyReport> {
    let report = evaluate_memory_proposal_policy(proposal, descriptor);
    if report.accepted {
        Ok(report)
    } else {
        let first = report
            .diagnostics
            .first()
            .expect("rejected policy report has diagnostic");
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{}: {}", first.reason_code, first.message),
        ))
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

/// Bounded per-wake activity digest used by the post-wake capture producer.
///
/// The digest is intentionally not a raw transcript. TypeScript builds it from
/// the warm post-wake event stream, then Rust persists it for scheduled
/// background review. Capture Phase 1 validates `profile_dense` proposals first;
/// `session_memory` and `roleplay_lore` remain gated expansion targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionActivityDigest {
    pub digest_id: String,
    pub profile_id: ProfileId,
    pub session_id: SessionId,
    pub wake_id: String,
    pub source: String,
    pub summary_text: String,
    pub event_counts_json: Value,
    pub tool_calls_json: Value,
    pub signals_json: Value,
    pub completion_summary: Option<String>,
    pub allowed_capture_spaces: Vec<MemorySpaceId>,
    pub created_at: IsoTimestamp,
    pub retention_until: Option<IsoTimestamp>,
    pub reviewed_at: Option<IsoTimestamp>,
}

impl SessionActivityDigest {
    pub fn validate(&self) -> CoreResult<()> {
        validate_identifier("session activity digest id", &self.digest_id)?;
        if self.wake_id.trim().is_empty() {
            return invalid("session activity digest wake_id must not be empty");
        }
        if self.source.trim().is_empty() {
            return invalid("session activity digest source must not be empty");
        }
        if self.summary_text.trim().is_empty() {
            return invalid("session activity digest summary_text must not be empty");
        }
        for space in &self.allowed_capture_spaces {
            space.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionActivityDigestQuery {
    pub profile_id: Option<ProfileId>,
    pub session_id: Option<SessionId>,
    pub wake_id: Option<String>,
    pub include_reviewed: bool,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Durable record for a derived context-compaction summary.
///
/// Compaction artifacts are not raw transcript storage and are not ordinary
/// session memory. They preserve provenance, model metadata, token estimates,
/// and strategy decisions so future context strategies can decide whether and
/// how to project them into model context while keeping source transcript
/// history intact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContextCompactionArtifact {
    pub artifact_id: String,
    pub session_id: SessionId,
    pub branch_id: Option<crate::ConversationBranchId>,
    pub strategy_id: String,
    pub source_refs_json: Value,
    pub provider_metadata_json: Value,
    pub estimate_before_json: Value,
    pub estimate_after_json: Option<Value>,
    pub summary_text: String,
    pub enters_future_context: bool,
    pub context_policy: String,
    pub metadata_json: Value,
    pub created_at: IsoTimestamp,
    pub updated_at: IsoTimestamp,
}

impl ContextCompactionArtifact {
    pub fn validate(&self) -> CoreResult<()> {
        validate_identifier("context compaction artifact id", &self.artifact_id)?;
        validate_identifier("context compaction strategy id", &self.strategy_id)?;
        if self.summary_text.trim().is_empty() {
            return invalid("context compaction artifact summary_text must not be empty");
        }
        if self.context_policy.trim().is_empty() {
            return invalid("context compaction artifact context_policy must not be empty");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCompactionArtifactQuery {
    pub session_id: Option<SessionId>,
    pub branch_id: Option<crate::ConversationBranchId>,
    pub strategy_id: Option<String>,
    pub enters_future_context: Option<bool>,
    pub latest_only: bool,
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

pub fn evaluate_memory_governance_decision_policy(
    decision: &MemoryGovernanceDecisionInput,
) -> MemoryPolicyReport {
    let mut report = MemoryPolicyReport::accepted();
    if validate_identifier("memory governance decision id", &decision.decision_id).is_err() {
        report.push_rejection(
            "memory_policy_decision_id_invalid",
            "memory governance decision id must be a lowercase snake_case identifier",
        );
    }
    if validate_identifier("memory governance proposal id", &decision.proposal_id).is_err() {
        report.push_rejection(
            "memory_policy_decision_proposal_id_invalid",
            "memory governance proposal id must be a lowercase snake_case identifier",
        );
    }
    if decision.actor.trim().is_empty() {
        report.push_rejection(
            "memory_policy_decision_actor_required",
            "memory governance actor must not be empty",
        );
    }
    if let Some(confidence) = decision.confidence {
        if let Err(error) = validate_confidence(confidence) {
            report.push_rejection(
                "memory_policy_decision_confidence_invalid",
                format!("memory governance confidence is invalid: {error}"),
            );
        }
    }
    for evidence in &decision.evidence_refs {
        if evidence.ref_id.trim().is_empty() {
            report.push_rejection(
                "memory_policy_decision_evidence_ref_invalid",
                "memory governance evidence ref_id must not be empty",
            );
        }
    }
    report
}

pub fn validate_memory_governance_decision_policy(
    decision: &MemoryGovernanceDecisionInput,
) -> CoreResult<MemoryPolicyReport> {
    let report = evaluate_memory_governance_decision_policy(decision);
    if report.accepted {
        Ok(report)
    } else {
        let first = report
            .diagnostics
            .first()
            .expect("rejected policy report has diagnostic");
        Err(CoreError::new(
            CoreErrorKind::InvalidInput,
            format!("{}: {}", first.reason_code, first.message),
        ))
    }
}

pub fn evaluate_memory_governance_transition_policy(
    current: MemoryProposalReviewStatus,
    decision: MemoryGovernanceDecisionKind,
) -> MemoryPolicyReport {
    let allowed = match (current, decision) {
        (_, MemoryGovernanceDecisionKind::RoutedToReview) => false,
        (MemoryProposalReviewStatus::PendingReview, MemoryGovernanceDecisionKind::Approved) => true,
        (MemoryProposalReviewStatus::PendingReview, MemoryGovernanceDecisionKind::Rejected) => true,
        (MemoryProposalReviewStatus::Approved, MemoryGovernanceDecisionKind::Applied) => true,
        _ => false,
    };
    if allowed {
        MemoryPolicyReport::accepted()
    } else {
        MemoryPolicyReport::reject(
            "memory_policy_transition_rejected",
            format!("memory governance decision {decision:?} is not allowed from {current:?}"),
        )
    }
}

pub fn validate_memory_governance_transition_policy(
    current: MemoryProposalReviewStatus,
    decision: MemoryGovernanceDecisionKind,
) -> CoreResult<MemoryPolicyReport> {
    let report = evaluate_memory_governance_transition_policy(current, decision);
    if report.accepted {
        Ok(report)
    } else {
        let first = report
            .diagnostics
            .first()
            .expect("rejected policy report has diagnostic");
        Err(CoreError::new(
            CoreErrorKind::ActionRejected,
            format!("{}: {}", first.reason_code, first.message),
        ))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CuratorGovernanceAction {
    PreviewCandidate,
    ApproveCandidate,
    ApplyCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CuratorStoredCandidateStatus {
    Proposed,
    Previewed,
    Approved,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CuratorCandidateLifecycleState {
    Active,
    Stale,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuratorGovernanceCandidateSnapshot {
    pub candidate_id: String,
    pub fingerprint: String,
    pub status: CuratorStoredCandidateStatus,
    pub lifecycle_state: Option<CuratorCandidateLifecycleState>,
    pub lifecycle_reason_code: Option<String>,
    pub expires_at: Option<IsoTimestamp>,
    pub approval_fingerprint: Option<String>,
    pub source_current: bool,
    pub source_current_reason_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuratorGovernancePlanInput {
    pub action: CuratorGovernanceAction,
    pub candidate: CuratorGovernanceCandidateSnapshot,
    pub now: IsoTimestamp,
    pub actor: Option<String>,
    pub reason: Option<String>,
    pub dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CuratorGovernancePlan {
    pub accepted: bool,
    pub action: CuratorGovernanceAction,
    pub candidate_id: String,
    pub audit_ref: Option<String>,
    pub receipt_id: String,
    pub resulting_status: Option<CuratorStoredCandidateStatus>,
    pub diagnostics: Vec<MemoryPolicyDiagnostic>,
}

pub fn plan_curator_governance_transition(
    input: CuratorGovernancePlanInput,
) -> CuratorGovernancePlan {
    let mut report = MemoryPolicyReport::accepted();
    let candidate = &input.candidate;
    if candidate.candidate_id.trim().is_empty() {
        report.push_rejection(
            "curator_candidate_id_required",
            "curator candidate id must not be empty",
        );
    }
    if candidate.fingerprint.trim().is_empty() {
        report.push_rejection(
            "curator_candidate_fingerprint_required",
            "curator candidate fingerprint must not be empty",
        );
    }
    if candidate.lifecycle_state == Some(CuratorCandidateLifecycleState::Archived) {
        report.push_rejection(
            "curator_candidate_archived",
            "archived curator candidates cannot be previewed, approved, or applied",
        );
    }
    if !candidate.source_current {
        report.push_rejection(
            candidate
                .source_current_reason_code
                .clone()
                .unwrap_or_else(|| "curator_candidate_stale".to_string()),
            "curator candidate source refs are no longer current",
        );
    }
    if matches!(
        input.action,
        CuratorGovernanceAction::ApproveCandidate | CuratorGovernanceAction::ApplyCandidate
    ) {
        if let Some(expires_at) = &candidate.expires_at {
            if expires_at <= &input.now {
                report.push_rejection(
                    "curator_candidate_expired",
                    "expired curator candidates cannot be approved or applied",
                );
            }
        }
    }
    if input.action == CuratorGovernanceAction::ApplyCandidate && !input.dry_run {
        if candidate.status != CuratorStoredCandidateStatus::Approved {
            report.push_rejection(
                "curator_candidate_not_approved",
                "curator candidate must be approved before apply",
            );
        }
        match &candidate.approval_fingerprint {
            Some(approval_fingerprint) if approval_fingerprint == &candidate.fingerprint => {}
            Some(_) => report.push_rejection(
                "curator_approval_stale",
                "curator approval fingerprint no longer matches the candidate",
            ),
            None => report.push_rejection(
                "curator_candidate_not_approved",
                "curator candidate must be approved before apply",
            ),
        }
    }
    let resulting_status = if report.accepted {
        Some(match input.action {
            CuratorGovernanceAction::PreviewCandidate => CuratorStoredCandidateStatus::Previewed,
            CuratorGovernanceAction::ApproveCandidate => CuratorStoredCandidateStatus::Approved,
            CuratorGovernanceAction::ApplyCandidate if input.dry_run => {
                CuratorStoredCandidateStatus::Previewed
            }
            CuratorGovernanceAction::ApplyCandidate => CuratorStoredCandidateStatus::Applied,
        })
    } else {
        None
    };
    let audit_kind = match input.action {
        CuratorGovernanceAction::PreviewCandidate => "curator-preview",
        CuratorGovernanceAction::ApproveCandidate => "curator-approval",
        CuratorGovernanceAction::ApplyCandidate if input.dry_run => "curator-preview",
        CuratorGovernanceAction::ApplyCandidate => "curator-apply",
    };
    let receipt_hash = stable_hash64(&format!(
        "{:?}:{}:{}:{}:{}",
        input.action,
        candidate.candidate_id,
        candidate.fingerprint,
        input.actor.as_deref().unwrap_or(""),
        input.reason.as_deref().unwrap_or("")
    ));
    CuratorGovernancePlan {
        accepted: report.accepted,
        action: input.action,
        candidate_id: candidate.candidate_id.clone(),
        audit_ref: report
            .accepted
            .then(|| format!("{audit_kind}:{}", candidate.candidate_id)),
        receipt_id: format!("curator-receipt:{audit_kind}:{receipt_hash:016x}"),
        resulting_status,
        diagnostics: report.diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureMemoryProposalPlanInput {
    pub run_id: String,
    pub profile_id: String,
    #[serde(default)]
    pub allowed_spaces: Vec<MemorySpaceId>,
    #[serde(default)]
    pub max_proposals: Option<u32>,
    #[serde(default)]
    pub candidates: Vec<CaptureMemoryProposalCandidate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureMemoryProposalCandidate {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub space_id: Option<MemorySpaceId>,
    #[serde(default)]
    pub operation: Option<MemoryOperation>,
    #[serde(default)]
    pub scope: Option<MemoryScope>,
    #[serde(default)]
    pub shape: Option<MemoryRecordShapeRef>,
    #[serde(default)]
    pub content: Option<Value>,
    #[serde(default)]
    pub evidence_refs: Vec<CaptureMemoryEvidenceRef>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default, alias = "durabilityRationale")]
    pub durability_rationale: Option<String>,
    #[serde(default, alias = "governancePolicy")]
    pub governance_policy: Option<MemoryGovernanceMode>,
    #[serde(default)]
    pub dedupe_key: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, alias = "memoryKey")]
    pub memory_key: Option<String>,
    #[serde(default, alias = "memoryContent")]
    pub memory_content: Option<String>,
    #[serde(default, alias = "replacesKey")]
    pub replaces_key: Option<String>,
    #[serde(default, alias = "expectedRevision")]
    pub expected_revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureMemoryEvidenceRef {
    #[serde(default, alias = "eventType")]
    pub event_type: Option<String>,
    #[serde(default, alias = "wakeId")]
    pub wake_id: Option<String>,
    #[serde(default, alias = "refId")]
    pub ref_id: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default, alias = "evidenceType")]
    pub evidence_type: Option<MemoryEvidenceKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureMemoryProposalPlan {
    pub proposals: Vec<MemoryProposalEnvelope>,
    pub rejected: Vec<CaptureMemoryProposalRejection>,
    pub skipped_reasons: Vec<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaptureMemoryProposalRejection {
    pub index: u32,
    pub reason_code: String,
    pub message: String,
}

pub fn plan_capture_memory_proposals(
    input: CaptureMemoryProposalPlanInput,
) -> CaptureMemoryProposalPlan {
    let allowed_spaces = if input.allowed_spaces.is_empty() {
        vec![MemorySpaceId::unchecked("profile_dense")]
    } else {
        input.allowed_spaces
    };
    let max_proposals = input.max_proposals.unwrap_or(8).clamp(1, 64) as usize;
    let mut proposals = Vec::new();
    let mut rejected = Vec::new();
    let mut truncated = false;
    for (index, candidate) in input.candidates.into_iter().enumerate() {
        if proposals.len() >= max_proposals {
            truncated = true;
            break;
        }
        match capture_candidate_to_memory_proposal(
            &input.run_id,
            &input.profile_id,
            candidate,
            &allowed_spaces,
        ) {
            Ok(proposal) => proposals.push(proposal),
            Err(rejection) => rejected.push(CaptureMemoryProposalRejection {
                index: index as u32,
                reason_code: rejection.reason_code,
                message: rejection.message,
            }),
        }
    }
    let skipped_reasons = if proposals.is_empty() {
        vec!["capture_no_supported_proposals".to_string()]
    } else {
        Vec::new()
    };
    CaptureMemoryProposalPlan {
        proposals,
        rejected,
        skipped_reasons,
        truncated,
    }
}

fn capture_candidate_to_memory_proposal(
    run_id: &str,
    profile_id: &str,
    candidate: CaptureMemoryProposalCandidate,
    allowed_spaces: &[MemorySpaceId],
) -> Result<MemoryProposalEnvelope, MemoryPolicyDiagnostic> {
    let proposal = if candidate.kind.as_deref().is_some_and(|kind| {
        matches!(
            kind,
            "dense_memory_add" | "dense_memory_replace" | "dense_memory_remove"
        )
    }) {
        legacy_dense_capture_candidate_to_proposal(run_id, profile_id, candidate)?
    } else {
        typed_capture_candidate_to_proposal(run_id, candidate)?
    };
    if !allowed_spaces
        .iter()
        .any(|space_id| *space_id == proposal.space_id)
    {
        return Err(MemoryPolicyDiagnostic {
            reason_code: "capture_space_not_allowed".to_string(),
            message: format!("capture space {} is not allowed", proposal.space_id),
        });
    }
    if proposal.space_id.as_str() != "profile_dense" {
        return Err(MemoryPolicyDiagnostic {
            reason_code: "capture_space_disabled".to_string(),
            message: format!(
                "capture space {} is not enabled for Rust capture proposal planning yet",
                proposal.space_id
            ),
        });
    }
    let descriptor = capture_profile_dense_descriptor();
    validate_memory_proposal_policy(&proposal, &descriptor).map_err(|error| {
        let text = error.to_string();
        let (reason_code, message) = policy_reason_from_error_text(&text);
        MemoryPolicyDiagnostic {
            reason_code,
            message,
        }
    })?;
    Ok(proposal)
}

fn typed_capture_candidate_to_proposal(
    run_id: &str,
    candidate: CaptureMemoryProposalCandidate,
) -> Result<MemoryProposalEnvelope, MemoryPolicyDiagnostic> {
    let space_id = candidate.space_id.ok_or_else(|| {
        capture_rejection(
            "capture_space_missing",
            "typed capture proposal requires space_id",
        )
    })?;
    let operation = candidate.operation.ok_or_else(|| {
        capture_rejection(
            "capture_operation_missing",
            "typed capture proposal requires operation",
        )
    })?;
    let scope = candidate.scope.ok_or_else(|| {
        capture_rejection(
            "capture_scope_missing",
            "typed capture proposal requires scope",
        )
    })?;
    let shape = candidate.shape.ok_or_else(|| {
        capture_rejection(
            "capture_shape_missing",
            "typed capture proposal requires shape",
        )
    })?;
    let content = candidate.content.ok_or_else(|| {
        capture_rejection(
            "capture_content_missing",
            "typed capture proposal requires content",
        )
    })?;
    let summary = normalized_capture_text(candidate.summary.as_deref())
        .unwrap_or_else(|| "capture proposal".to_string());
    let proposal_id = capture_proposal_id(run_id, candidate.id.as_deref(), &summary);
    let evidence_refs = capture_evidence_refs(run_id, candidate.evidence_refs);
    let confidence = candidate.confidence.unwrap_or(0.0).clamp(0.0, 1.0);
    let durability_rationale = normalized_capture_text(candidate.durability_rationale.as_deref())
        .ok_or_else(|| {
        capture_rejection(
            "capture_rationale_missing",
            "typed capture proposal requires durability_rationale",
        )
    })?;
    let dedupe_key = candidate.dedupe_key.or_else(|| {
        Some(capture_dedupe_key(&[
            space_id.as_str(),
            &format!("{operation:?}"),
            &format!("{:?}", scope.scope_type),
            scope.scope_id.as_str(),
            &stable_capture_json(&content),
        ]))
    });
    Ok(MemoryProposalEnvelope {
        proposal_id,
        space_id,
        operation,
        scope,
        shape,
        content,
        evidence_refs,
        confidence,
        durability_rationale: Some(durability_rationale),
        governance_mode: select_memory_governance_mode(
            candidate
                .governance_policy
                .unwrap_or(MemoryGovernanceMode::CuratorRoute),
            MemoryProposalSource::CaptureProducer,
        ),
        source: MemoryProposalSource::CaptureProducer,
        dedupe_key,
        created_at: None,
    })
}

fn legacy_dense_capture_candidate_to_proposal(
    run_id: &str,
    profile_id: &str,
    candidate: CaptureMemoryProposalCandidate,
) -> Result<MemoryProposalEnvelope, MemoryPolicyDiagnostic> {
    let kind = candidate.kind.as_deref().unwrap_or_default();
    let operation = match kind {
        "dense_memory_add" => MemoryOperation::Add,
        "dense_memory_replace" => MemoryOperation::Replace,
        "dense_memory_remove" => MemoryOperation::Remove,
        _ => {
            return Err(capture_rejection(
                "capture_legacy_kind_unsupported",
                format!("unsupported legacy capture kind {kind}"),
            ))
        }
    };
    let key = normalized_capture_text(
        candidate
            .memory_key
            .as_deref()
            .or(candidate.replaces_key.as_deref()),
    )
    .ok_or_else(|| {
        capture_rejection(
            "capture_memory_key_missing",
            format!("{kind} requires memoryKey or replacesKey"),
        )
    })?;
    let summary = normalized_capture_text(candidate.summary.as_deref())
        .unwrap_or_else(|| "legacy dense memory capture".to_string());
    let mut content = serde_json::Map::new();
    content.insert("key".to_string(), Value::String(key.clone()));
    if operation != MemoryOperation::Remove {
        let body =
            normalized_capture_text(candidate.memory_content.as_deref()).ok_or_else(|| {
                capture_rejection(
                    "capture_memory_content_missing",
                    format!("{kind} requires memoryContent"),
                )
            })?;
        content.insert("content".to_string(), Value::String(body));
    }
    if let Some(replaces_key) = normalized_capture_text(candidate.replaces_key.as_deref()) {
        content.insert("replaces_key".to_string(), Value::String(replaces_key));
    }
    if let Some(expected_revision) = candidate.expected_revision {
        content.insert(
            "expected_revision".to_string(),
            Value::Number(expected_revision.into()),
        );
    }
    content.insert(
        "metadata_json".to_string(),
        Value::Object({
            let mut metadata = serde_json::Map::new();
            metadata.insert(
                "capture_summary".to_string(),
                Value::String(summary.clone()),
            );
            metadata.insert(
                "legacy_capture_kind".to_string(),
                Value::String(kind.to_string()),
            );
            metadata
        }),
    );
    Ok(MemoryProposalEnvelope {
        proposal_id: capture_proposal_id(run_id, candidate.id.as_deref(), &summary),
        space_id: MemorySpaceId::unchecked("profile_dense"),
        operation,
        scope: MemoryScope {
            scope_type: MemoryScopeType::Profile,
            scope_id: profile_id.to_string(),
        },
        shape: MemoryRecordShapeRef {
            shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
            version: 1,
        },
        content: Value::Object(content),
        evidence_refs: capture_evidence_refs(run_id, candidate.evidence_refs),
        confidence: candidate.confidence.unwrap_or(0.0).clamp(0.0, 1.0),
        durability_rationale: normalized_capture_text(candidate.durability_rationale.as_deref()),
        governance_mode: MemoryGovernanceMode::CuratorRoute,
        source: MemoryProposalSource::CaptureProducer,
        dedupe_key: Some(capture_dedupe_key(&[
            "profile_dense",
            &format!("{operation:?}"),
            "profile",
            profile_id,
            key.as_str(),
        ])),
        created_at: None,
    })
}

fn capture_profile_dense_descriptor() -> MemorySpaceDescriptor {
    MemorySpaceDescriptor {
        space_id: MemorySpaceId::unchecked("profile_dense"),
        schema_version: 1,
        module_id: Some("runtime_memory".to_string()),
        description: "Capture planner profile dense memory descriptor.".to_string(),
        record_shapes: vec![MemoryRecordShapeDescriptor {
            shape_id: MemoryRecordShapeId::unchecked("profile_dense_item"),
            version: 1,
            description: "Keyed profile dense memory item.".to_string(),
            fields: vec![
                descriptor_field("key", MemoryFieldType::String, true),
                descriptor_field("content", MemoryFieldType::Markdown, false),
                descriptor_field("expected_revision", MemoryFieldType::Integer, false),
                descriptor_field("metadata_json", MemoryFieldType::Json, false),
            ],
        }],
        scope_model: MemoryScopeModel {
            allowed_scopes: vec![MemoryScopeType::Profile, MemoryScopeType::User],
            primary_scope: MemoryScopeType::Profile,
        },
        visibility_model: MemoryVisibilityModel::ProfileLocal,
        retrieval_strategies: vec![MemoryRetrievalStrategy::DirectLookup],
        indexing: MemoryIndexingPolicy {
            required_capabilities: vec!["profile_target_key_lookup".to_string()],
            optional_capabilities: vec![],
        },
        prompt_policy: MemoryPromptPolicy::SummaryContext,
        write_policy: MemoryWritePolicy {
            default_mode: MemoryGovernanceMode::CuratorRoute,
            operation_policies: vec![
                descriptor_op_policy(
                    MemoryOperation::Add,
                    MemoryGovernanceMode::CuratorRoute,
                    false,
                ),
                descriptor_op_policy(
                    MemoryOperation::Replace,
                    MemoryGovernanceMode::CuratorRoute,
                    true,
                ),
                descriptor_op_policy(
                    MemoryOperation::Remove,
                    MemoryGovernanceMode::CuratorRoute,
                    true,
                ),
                descriptor_op_policy(
                    MemoryOperation::CandidateOnly,
                    MemoryGovernanceMode::CuratorRoute,
                    false,
                ),
            ],
        },
        operations: vec![
            MemoryOperation::Read,
            MemoryOperation::List,
            MemoryOperation::Add,
            MemoryOperation::Replace,
            MemoryOperation::Remove,
            MemoryOperation::CandidateOnly,
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

fn capture_evidence_refs(
    run_id: &str,
    refs: Vec<CaptureMemoryEvidenceRef>,
) -> Vec<MemoryEvidenceRef> {
    let mut mapped: Vec<MemoryEvidenceRef> = refs
        .iter()
        .map(|evidence| MemoryEvidenceRef {
            evidence_type: evidence
                .evidence_type
                .or_else(|| evidence_kind_from_event(evidence.event_type.as_deref()))
                .unwrap_or(MemoryEvidenceKind::Event),
            ref_id: normalized_capture_text(evidence.ref_id.as_deref())
                .or_else(|| normalized_capture_text(evidence.wake_id.as_deref()))
                .unwrap_or_else(|| format!("{run_id}:capture_producer")),
            label: normalized_capture_text(evidence.summary.as_deref()),
        })
        .collect();
    if !mapped
        .iter()
        .any(|evidence| evidence.evidence_type == MemoryEvidenceKind::Wake)
    {
        mapped.insert(
            0,
            MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: refs
                    .iter()
                    .find_map(|evidence| normalized_capture_text(evidence.wake_id.as_deref()))
                    .unwrap_or_else(|| format!("{run_id}:wake")),
                label: Some("capture producer wake evidence".to_string()),
            },
        );
    }
    mapped
}

fn evidence_kind_from_event(event_type: Option<&str>) -> Option<MemoryEvidenceKind> {
    let event_type = event_type?;
    if event_type.contains("correction") {
        Some(MemoryEvidenceKind::UserCorrection)
    } else if event_type.contains("tool") {
        Some(MemoryEvidenceKind::ToolCall)
    } else if event_type.contains("transcript") {
        Some(MemoryEvidenceKind::Transcript)
    } else if event_type.contains("wake") {
        Some(MemoryEvidenceKind::Wake)
    } else {
        Some(MemoryEvidenceKind::Event)
    }
}

fn capture_proposal_id(run_id: &str, explicit_id: Option<&str>, summary: &str) -> String {
    let raw = explicit_id
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| summary);
    let normalized = snake_identifier(raw);
    if is_valid_capture_identifier(&normalized) {
        normalized
    } else {
        format!("cap_{:016x}", stable_hash64(&format!("{run_id}:{raw}")))
    }
}

fn capture_dedupe_key(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| part.trim().to_lowercase().replace(char::is_whitespace, " "))
        .collect::<Vec<_>>()
        .join(":")
}

fn stable_capture_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn normalized_capture_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn snake_identifier(value: &str) -> String {
    let mut output = String::new();
    let mut last_was_underscore = false;
    for ch in value.trim().to_lowercase().chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            output.push(ch);
            last_was_underscore = false;
        } else if !last_was_underscore {
            output.push('_');
            last_was_underscore = true;
        }
    }
    output.trim_matches('_').to_string()
}

fn is_valid_capture_identifier(value: &str) -> bool {
    validate_identifier("capture proposal id", value).is_ok()
}

fn stable_hash64(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn capture_rejection(
    reason_code: impl Into<String>,
    message: impl Into<String>,
) -> MemoryPolicyDiagnostic {
    MemoryPolicyDiagnostic {
        reason_code: reason_code.into(),
        message: message.into(),
    }
}

fn policy_reason_from_error_text(text: &str) -> (String, String) {
    if let Some(start) = text.find("memory_policy_") {
        let policy_text = &text[start..];
        if let Some((reason_code, message)) = policy_text.split_once(':') {
            return (reason_code.trim().to_string(), message.trim().to_string());
        }
        return (policy_text.trim().to_string(), text.to_string());
    }
    ("memory_policy_rejected".to_string(), text.to_string())
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
    fn session_memory_descriptor_declares_shapes_scopes_and_conservative_operations() {
        let descriptor = session_memory_descriptor();
        descriptor
            .validate()
            .expect("session memory descriptor is valid");

        assert_eq!(descriptor.space_id.as_str(), "session_memory");
        assert_eq!(descriptor.module_id.as_deref(), Some("runtime_memory"));
        assert!(
            descriptor
                .description
                .contains("not Den memory and not transcript storage"),
            "descriptor should document the Crew-memory boundary"
        );
        assert_eq!(
            descriptor.scope_model.allowed_scopes,
            vec![
                MemoryScopeType::Session,
                MemoryScopeType::ConversationBranch,
            ]
        );
        assert_eq!(
            descriptor.retrieval_strategies,
            vec![
                MemoryRetrievalStrategy::DirectLookup,
                MemoryRetrievalStrategy::Recency,
                MemoryRetrievalStrategy::BranchAware,
                MemoryRetrievalStrategy::QuerySearch,
            ]
        );
        assert_eq!(
            descriptor.operations,
            vec![
                MemoryOperation::Read,
                MemoryOperation::List,
                MemoryOperation::Add,
                MemoryOperation::Replace,
                MemoryOperation::Merge,
                MemoryOperation::Supersede,
                MemoryOperation::Archive,
            ]
        );
        assert_eq!(descriptor.retention_policy, MemoryRetentionPolicy::Compact);
        assert_eq!(
            descriptor.conflict_policy,
            MemoryConflictPolicy::Supersession
        );

        for shape_id in [
            "session_fact",
            "session_summary",
            "branch_summary",
            "user_choice",
        ] {
            let shape = descriptor
                .record_shapes
                .iter()
                .find(|shape| shape.shape_id.as_str() == shape_id)
                .unwrap_or_else(|| panic!("missing session_memory shape {shape_id}"));
            assert_eq!(shape.version, 1);
            assert!(shape
                .fields
                .iter()
                .any(|field| field.field_name == "record_id" && field.required));
            assert!(shape
                .fields
                .iter()
                .any(|field| field.field_name == "created_at" && field.required));
            assert!(shape
                .fields
                .iter()
                .any(|field| field.field_name == "updated_at" && field.required));
        }

        assert_required_fields(
            &descriptor,
            "session_fact",
            &[
                "record_id",
                "content",
                "fact_kind",
                "confidence",
                "source_summary",
                "created_at",
                "updated_at",
            ],
        );
        assert_required_fields(
            &descriptor,
            "session_summary",
            &[
                "record_id",
                "summary",
                "coverage_start",
                "coverage_end",
                "summary_kind",
                "created_at",
                "updated_at",
            ],
        );
        assert_required_fields(
            &descriptor,
            "branch_summary",
            &[
                "record_id",
                "summary",
                "branch_id",
                "head_message_id",
                "coverage_start",
                "coverage_end",
                "created_at",
                "updated_at",
            ],
        );
        assert_required_fields(
            &descriptor,
            "user_choice",
            &[
                "record_id",
                "choice",
                "choice_kind",
                "chosen_at",
                "status",
                "created_at",
                "updated_at",
            ],
        );

        assert_operation_policy(
            &descriptor,
            MemoryOperation::Add,
            MemoryGovernanceMode::Candidate,
            false,
        );
        assert_operation_policy(
            &descriptor,
            MemoryOperation::Replace,
            MemoryGovernanceMode::CuratorRoute,
            true,
        );
        assert_operation_policy(
            &descriptor,
            MemoryOperation::Merge,
            MemoryGovernanceMode::CuratorRoute,
            true,
        );
        assert_operation_policy(
            &descriptor,
            MemoryOperation::Supersede,
            MemoryGovernanceMode::CuratorRoute,
            true,
        );
        assert_operation_policy(
            &descriptor,
            MemoryOperation::Archive,
            MemoryGovernanceMode::ManualReview,
            true,
        );
    }

    #[test]
    fn session_memory_proposal_validation_rejects_invalid_scope_operation_and_shape() {
        let descriptor = session_memory_descriptor();
        let proposal = valid_session_memory_proposal(
            MemoryOperation::Add,
            MemoryScopeType::Session,
            "session_fact",
        );
        proposal
            .validate_for_descriptor(&descriptor)
            .expect("session memory proposal matches descriptor");

        let mut bad_scope = proposal.clone();
        bad_scope.scope.scope_type = MemoryScopeType::Profile;
        assert!(bad_scope.validate_for_descriptor(&descriptor).is_err());

        let mut bad_operation = proposal.clone();
        bad_operation.operation = MemoryOperation::Remove;
        assert!(bad_operation.validate_for_descriptor(&descriptor).is_err());

        let mut bad_shape = proposal.clone();
        bad_shape.shape.shape_id = MemoryRecordShapeId::unchecked("transcript_message");
        assert!(bad_shape.validate_for_descriptor(&descriptor).is_err());

        let branch_summary = valid_session_memory_proposal(
            MemoryOperation::Supersede,
            MemoryScopeType::ConversationBranch,
            "branch_summary",
        );
        branch_summary
            .validate_for_descriptor(&descriptor)
            .expect("branch summary proposal matches descriptor");
    }

    #[test]
    fn memory_policy_port_emits_stable_proposal_diagnostics() {
        let descriptor = session_memory_descriptor();
        let mut proposal = valid_session_memory_proposal(
            MemoryOperation::Replace,
            MemoryScopeType::Session,
            "session_fact",
        );
        proposal
            .content
            .as_object_mut()
            .unwrap()
            .remove("expected_revision");

        let report = evaluate_memory_proposal_policy(&proposal, &descriptor);
        assert!(!report.accepted);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.reason_code == "memory_policy_expected_revision_required"
        }));

        let error = validate_memory_proposal_policy(&proposal, &descriptor)
            .expect_err("policy rejects missing revision");
        assert!(error
            .to_string()
            .contains("memory_policy_expected_revision_required"));
    }

    #[test]
    fn memory_policy_port_keeps_den_memory_as_external_import_evidence() {
        let descriptor = session_memory_descriptor();
        let mut proposal = valid_session_memory_proposal(
            MemoryOperation::Add,
            MemoryScopeType::Session,
            "session_fact",
        );
        proposal.source = MemoryProposalSource::DenMemoryImport;

        let report = evaluate_memory_proposal_policy(&proposal, &descriptor);
        assert!(!report.accepted);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.reason_code == "memory_policy_den_import_evidence_required"
        }));

        proposal.evidence_refs.push(MemoryEvidenceRef {
            evidence_type: MemoryEvidenceKind::DenMemory,
            ref_id: "den-memory-1".to_string(),
            label: Some("external Den memory evidence".to_string()),
        });
        validate_memory_proposal_policy(&proposal, &descriptor)
            .expect("Den memory import is accepted only as evidenced Crew proposal");
    }

    #[test]
    fn memory_policy_port_validates_decisions_and_transitions() {
        let mut decision = MemoryGovernanceDecisionInput {
            decision_id: "decision_one".to_string(),
            proposal_id: "proposal_one".to_string(),
            decision: MemoryGovernanceDecisionKind::Approved,
            actor: "curator".to_string(),
            source: MemoryProposalSource::Human,
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Ui,
                ref_id: "review-1".to_string(),
                label: None,
            }],
            policy_mode: MemoryGovernanceMode::ManualReview,
            confidence: Some(0.8),
            message: Some("Looks stable.".to_string()),
            resulting_revision: None,
            decided_at: Some("2026-07-08T00:00:00Z".to_string()),
        };
        validate_memory_governance_decision_policy(&decision)
            .expect("decision policy accepts valid decision");

        decision.actor.clear();
        let report = evaluate_memory_governance_decision_policy(&decision);
        assert!(!report.accepted);
        assert_eq!(
            report.first_reason_code(),
            Some("memory_policy_decision_actor_required")
        );

        validate_memory_governance_transition_policy(
            MemoryProposalReviewStatus::PendingReview,
            MemoryGovernanceDecisionKind::Approved,
        )
        .expect("pending proposals can be approved");

        let rejected = validate_memory_governance_transition_policy(
            MemoryProposalReviewStatus::Rejected,
            MemoryGovernanceDecisionKind::Applied,
        )
        .expect_err("rejected proposals cannot be applied");
        assert!(rejected
            .to_string()
            .contains("memory_policy_transition_rejected"));
    }

    #[test]
    fn memory_policy_selection_is_source_aware() {
        assert_eq!(
            select_memory_governance_mode(
                MemoryGovernanceMode::DirectWrite,
                MemoryProposalSource::CaptureProducer
            ),
            MemoryGovernanceMode::CuratorRoute
        );
        assert_eq!(
            select_memory_governance_mode(
                MemoryGovernanceMode::DirectWrite,
                MemoryProposalSource::Human
            ),
            MemoryGovernanceMode::DirectWrite
        );
    }

    #[test]
    fn curator_governance_planner_enforces_state_before_mutation() {
        let candidate = CuratorGovernanceCandidateSnapshot {
            candidate_id: "curator:batch-1:patch-managed".to_string(),
            fingerprint: "candidate-fingerprint".to_string(),
            status: CuratorStoredCandidateStatus::Proposed,
            lifecycle_state: Some(CuratorCandidateLifecycleState::Active),
            lifecycle_reason_code: None,
            expires_at: Some("2026-07-09T00:00:00Z".to_string()),
            approval_fingerprint: None,
            source_current: true,
            source_current_reason_code: None,
        };

        let denied = plan_curator_governance_transition(CuratorGovernancePlanInput {
            action: CuratorGovernanceAction::ApplyCandidate,
            candidate: candidate.clone(),
            now: "2026-07-08T00:00:00Z".to_string(),
            actor: Some("curator".to_string()),
            reason: Some("apply".to_string()),
            dry_run: false,
        });
        assert!(!denied.accepted);
        assert_eq!(
            denied
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.reason_code.as_str()),
            Some("curator_candidate_not_approved")
        );

        let mut approved = candidate;
        approved.status = CuratorStoredCandidateStatus::Approved;
        approved.approval_fingerprint = Some(approved.fingerprint.clone());
        let accepted = plan_curator_governance_transition(CuratorGovernancePlanInput {
            action: CuratorGovernanceAction::ApplyCandidate,
            candidate: approved,
            now: "2026-07-08T00:00:00Z".to_string(),
            actor: Some("curator".to_string()),
            reason: Some("apply".to_string()),
            dry_run: false,
        });
        assert!(accepted.accepted);
        assert_eq!(
            accepted.resulting_status,
            Some(CuratorStoredCandidateStatus::Applied)
        );
        assert_eq!(
            accepted.audit_ref.as_deref(),
            Some("curator-apply:curator:batch-1:patch-managed")
        );

        let stale = plan_curator_governance_transition(CuratorGovernancePlanInput {
            action: CuratorGovernanceAction::ApproveCandidate,
            candidate: CuratorGovernanceCandidateSnapshot {
                source_current: false,
                source_current_reason_code: Some("curator_candidate_stale".to_string()),
                ..CuratorGovernanceCandidateSnapshot {
                    candidate_id: "curator:batch-1:stale".to_string(),
                    fingerprint: "candidate-fingerprint".to_string(),
                    status: CuratorStoredCandidateStatus::Proposed,
                    lifecycle_state: Some(CuratorCandidateLifecycleState::Active),
                    lifecycle_reason_code: None,
                    expires_at: None,
                    approval_fingerprint: None,
                    source_current: true,
                    source_current_reason_code: None,
                }
            },
            now: "2026-07-08T00:00:00Z".to_string(),
            actor: Some("curator".to_string()),
            reason: Some("approve".to_string()),
            dry_run: false,
        });
        assert_eq!(
            stale
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.reason_code.as_str()),
            Some("curator_candidate_stale")
        );
    }

    #[test]
    fn capture_planner_accepts_profile_dense_and_reports_disabled_spaces() {
        let plan = plan_capture_memory_proposals(CaptureMemoryProposalPlanInput {
            run_id: "capture_run_one".to_string(),
            profile_id: "profile-alpha".to_string(),
            allowed_spaces: vec![
                MemorySpaceId::unchecked("profile_dense"),
                MemorySpaceId::unchecked("session_memory"),
            ],
            max_proposals: None,
            candidates: vec![
                CaptureMemoryProposalCandidate {
                    id: Some("remember_review_style".to_string()),
                    summary: Some("Remember compact review style.".to_string()),
                    kind: Some("dense_memory_add".to_string()),
                    memory_key: Some("review_style".to_string()),
                    memory_content: Some("Prefers compact review summaries.".to_string()),
                    confidence: Some(0.9),
                    durability_rationale: Some(
                        "Stable profile preference for future reviews.".to_string(),
                    ),
                    evidence_refs: vec![CaptureMemoryEvidenceRef {
                        event_type: Some("user_correction".to_string()),
                        wake_id: Some("wake-one".to_string()),
                        ref_id: None,
                        summary: Some("User corrected review style.".to_string()),
                        evidence_type: None,
                    }],
                    ..empty_capture_candidate()
                },
                CaptureMemoryProposalCandidate {
                    id: Some("session_memory_candidate".to_string()),
                    summary: Some("Remember session fact.".to_string()),
                    space_id: Some(MemorySpaceId::unchecked("session_memory")),
                    operation: Some(MemoryOperation::Add),
                    scope: Some(MemoryScope {
                        scope_type: MemoryScopeType::Session,
                        scope_id: "session-alpha".to_string(),
                    }),
                    shape: Some(MemoryRecordShapeRef {
                        shape_id: MemoryRecordShapeId::unchecked("session_fact"),
                        version: 1,
                    }),
                    content: Some(json!({
                        "record_id": "session-fact-one",
                        "content": "The user prefers compact reviews.",
                        "fact_kind": "preference",
                        "confidence": 0.9,
                        "source_summary": "User corrected review style.",
                        "created_at": "2026-07-08T00:00:00Z",
                        "updated_at": "2026-07-08T00:00:00Z"
                    })),
                    evidence_refs: vec![CaptureMemoryEvidenceRef {
                        event_type: Some("wake".to_string()),
                        wake_id: Some("wake-one".to_string()),
                        ref_id: None,
                        summary: None,
                        evidence_type: None,
                    }],
                    confidence: Some(0.8),
                    durability_rationale: Some("Stable session fact.".to_string()),
                    governance_policy: Some(MemoryGovernanceMode::Candidate),
                    ..empty_capture_candidate()
                },
            ],
        });
        assert_eq!(plan.proposals.len(), 1);
        assert_eq!(plan.proposals[0].proposal_id, "remember_review_style");
        assert_eq!(plan.proposals[0].space_id.as_str(), "profile_dense");
        assert_eq!(
            plan.proposals[0].governance_mode,
            MemoryGovernanceMode::CuratorRoute
        );
        assert!(plan.proposals[0]
            .evidence_refs
            .iter()
            .any(|evidence| evidence.evidence_type == MemoryEvidenceKind::Wake));
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(plan.rejected[0].reason_code, "capture_space_disabled");
    }

    #[test]
    fn capture_planner_reports_policy_diagnostics_and_truncation() {
        let plan = plan_capture_memory_proposals(CaptureMemoryProposalPlanInput {
            run_id: "capture_run_two".to_string(),
            profile_id: "profile-alpha".to_string(),
            allowed_spaces: vec![MemorySpaceId::unchecked("profile_dense")],
            max_proposals: Some(1),
            candidates: vec![
                CaptureMemoryProposalCandidate {
                    id: Some("replace_without_revision".to_string()),
                    summary: Some("Replace memory without revision.".to_string()),
                    kind: Some("dense_memory_replace".to_string()),
                    memory_key: Some("review_style".to_string()),
                    memory_content: Some("Prefers concise summaries.".to_string()),
                    confidence: Some(0.8),
                    durability_rationale: Some("Stable correction.".to_string()),
                    evidence_refs: vec![CaptureMemoryEvidenceRef {
                        event_type: Some("wake".to_string()),
                        wake_id: Some("wake-two".to_string()),
                        ref_id: None,
                        summary: None,
                        evidence_type: None,
                    }],
                    ..empty_capture_candidate()
                },
                CaptureMemoryProposalCandidate {
                    id: Some("valid_add".to_string()),
                    summary: Some("Remember valid add.".to_string()),
                    kind: Some("dense_memory_add".to_string()),
                    memory_key: Some("valid_add".to_string()),
                    memory_content: Some("A valid add.".to_string()),
                    confidence: Some(0.8),
                    durability_rationale: Some("Stable.".to_string()),
                    evidence_refs: vec![CaptureMemoryEvidenceRef {
                        event_type: Some("wake".to_string()),
                        wake_id: Some("wake-two".to_string()),
                        ref_id: None,
                        summary: None,
                        evidence_type: None,
                    }],
                    ..empty_capture_candidate()
                },
                CaptureMemoryProposalCandidate {
                    id: Some("second_valid_add".to_string()),
                    summary: Some("Remember second valid add.".to_string()),
                    kind: Some("dense_memory_add".to_string()),
                    memory_key: Some("second_valid_add".to_string()),
                    memory_content: Some("Another valid add.".to_string()),
                    confidence: Some(0.8),
                    durability_rationale: Some("Stable.".to_string()),
                    evidence_refs: vec![CaptureMemoryEvidenceRef {
                        event_type: Some("wake".to_string()),
                        wake_id: Some("wake-two".to_string()),
                        ref_id: None,
                        summary: None,
                        evidence_type: None,
                    }],
                    ..empty_capture_candidate()
                },
            ],
        });
        assert_eq!(plan.proposals.len(), 1);
        assert!(plan.truncated);
        assert_eq!(plan.rejected.len(), 1);
        assert_eq!(
            plan.rejected[0].reason_code,
            "memory_policy_expected_revision_required"
        );
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
        session_memory_space_descriptor()
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

    fn empty_capture_candidate() -> CaptureMemoryProposalCandidate {
        CaptureMemoryProposalCandidate {
            id: None,
            summary: None,
            space_id: None,
            operation: None,
            scope: None,
            shape: None,
            content: None,
            evidence_refs: Vec::new(),
            confidence: None,
            durability_rationale: None,
            governance_policy: None,
            dedupe_key: None,
            kind: None,
            memory_key: None,
            memory_content: None,
            replaces_key: None,
            expected_revision: None,
        }
    }

    fn valid_session_memory_proposal(
        operation: MemoryOperation,
        scope_type: MemoryScopeType,
        shape_id: &str,
    ) -> MemoryProposalEnvelope {
        MemoryProposalEnvelope {
            proposal_id: "session_memory_proposal_one".to_string(),
            space_id: MemorySpaceId::unchecked("session_memory"),
            operation,
            scope: MemoryScope {
                scope_type,
                scope_id: match scope_type {
                    MemoryScopeType::ConversationBranch => "branch-alpha".to_string(),
                    _ => "session-alpha".to_string(),
                },
            },
            shape: MemoryRecordShapeRef {
                shape_id: MemoryRecordShapeId::unchecked(shape_id),
                version: 1,
            },
            content: match shape_id {
                "branch_summary" => json!({
                    "record_id": "branch-summary-one",
                    "summary": "The branch followed the quiet clue trail.",
                    "branch_id": "branch-alpha",
                    "head_message_id": "message-alpha",
                    "coverage_start": "message-root",
                    "coverage_end": "message-alpha",
                    "created_at": "2026-06-26T00:00:00Z",
                    "updated_at": "2026-06-26T00:00:00Z"
                }),
                _ => json!({
                    "record_id": "session-fact-one",
                    "content": "The user prefers slow-burn mystery pacing.",
                    "fact_kind": "preference",
                    "confidence": 0.9,
                    "source_summary": "User corrected pacing in the active session.",
                    "created_at": "2026-06-26T00:00:00Z",
                    "updated_at": "2026-06-26T00:00:00Z"
                }),
            },
            evidence_refs: vec![MemoryEvidenceRef {
                evidence_type: MemoryEvidenceKind::Wake,
                ref_id: "wake-session-1".to_string(),
                label: Some("Session wake".to_string()),
            }],
            confidence: 0.9,
            durability_rationale: Some(
                "Session-level memory should survive wakes without duplicating transcript storage."
                    .to_string(),
            ),
            governance_mode: MemoryGovernanceMode::Candidate,
            source: MemoryProposalSource::CaptureProducer,
            dedupe_key: Some("session_memory:preference:pacing".to_string()),
            created_at: Some("2026-06-26T00:00:00Z".to_string()),
        }
    }

    fn assert_required_fields(
        descriptor: &MemorySpaceDescriptor,
        shape_id: &str,
        expected_fields: &[&str],
    ) {
        let shape = descriptor
            .record_shapes
            .iter()
            .find(|shape| shape.shape_id.as_str() == shape_id)
            .unwrap_or_else(|| panic!("missing memory shape {shape_id}"));
        for expected_field in expected_fields {
            assert!(
                shape
                    .fields
                    .iter()
                    .any(|field| field.field_name == *expected_field && field.required),
                "shape {shape_id} missing required field {expected_field}"
            );
        }
    }

    fn assert_operation_policy(
        descriptor: &MemorySpaceDescriptor,
        operation: MemoryOperation,
        governance_mode: MemoryGovernanceMode,
        requires_expected_revision: bool,
    ) {
        let policy = descriptor
            .write_policy
            .operation_policies
            .iter()
            .find(|policy| policy.operation == operation)
            .unwrap_or_else(|| panic!("missing operation policy {operation:?}"));
        assert_eq!(policy.governance_mode, governance_mode);
        assert_eq!(
            policy.requires_expected_revision,
            requires_expected_revision
        );
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
