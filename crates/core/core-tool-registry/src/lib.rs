//! Portable tool registry metadata validation.
//!
//! This crate owns policy metadata that can be shared by Rust, generated
//! artifacts, admin surfaces, and TypeScript executable bindings. It
//! intentionally does not model JavaScript executor modules, factory names, MCP
//! clients, or other runtime binding details.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetadata {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub category: ToolCategory,
    #[serde(default)]
    pub toolsets: Vec<String>,
    #[serde(default)]
    pub surfaces: Vec<ToolSurface>,
    #[serde(default)]
    pub safety: Vec<ToolSafetyFlag>,
    pub output_shape: String,
    pub version: String,
    pub deprecated: Option<ToolDeprecation>,
    pub replacement: Option<String>,
    pub coexistence_note: Option<String>,
    pub collision_notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Local,
    Git,
    Patch,
    Web,
    Browser,
    Memory,
    Storage,
    Skills,
    Mcp,
    Coordination,
    Delegation,
    Planning,
    Diagnostics,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSurface {
    Brain,
    Mcp,
    Admin,
    Tui,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSafetyFlag {
    ReadOnly,
    WritesFiles,
    ExecutesProcess,
    WorkdirScoped,
    NetworkAccess,
    ExternalWrite,
    CoordinationAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDeprecation {
    pub reason: String,
    pub since: String,
    pub replacement: Option<String>,
    pub sunset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolInventoryStatus {
    Selected,
    NotRequested,
    ProfileDenied,
    SessionDenied,
    ResourceDenied,
    Deprecated,
    Missing,
    Shadowed,
    Collision,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDenialReasonCode {
    UnknownTool,
    UnknownToolset,
    ProfileDenied,
    SessionDenied,
    ResourceDenied,
    Deprecated,
    Shadowed,
    Collision,
    MissingExternalDependency,
    InvalidMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolMetadataDiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetadataDiagnostic {
    pub severity: ToolMetadataDiagnosticSeverity,
    pub code: String,
    pub tool_name: Option<String>,
    pub other_tool_name: Option<String>,
    pub path: Option<String>,
    pub message: String,
}

impl ToolMetadataDiagnostic {
    fn error(
        code: impl Into<String>,
        tool_name: Option<&str>,
        other_tool_name: Option<&str>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: ToolMetadataDiagnosticSeverity::Error,
            code: code.into(),
            tool_name: tool_name.map(ToOwned::to_owned),
            other_tool_name: other_tool_name.map(ToOwned::to_owned),
            path: Some(path.into()),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolMetadataValidationResult {
    pub diagnostics: Vec<ToolMetadataDiagnostic>,
}

impl ToolMetadataValidationResult {
    pub fn ok(&self) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ToolMetadataDiagnosticSeverity::Error)
    }
}

pub fn validate_tool_metadata_list(entries: &[ToolMetadata]) -> ToolMetadataValidationResult {
    let mut validator = ToolMetadataValidator::new(entries);
    validator.validate();
    ToolMetadataValidationResult {
        diagnostics: validator.diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadataPolicyValidationInput {
    #[serde(default)]
    pub tools: Vec<ToolMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMetadataPolicyValidationResult {
    pub ok: bool,
    pub diagnostics: Vec<ToolMetadataDiagnostic>,
}

pub fn validate_tool_metadata_policy(
    input: &ToolMetadataPolicyValidationInput,
) -> ToolMetadataPolicyValidationResult {
    let validation = validate_tool_metadata_list(&input.tools);
    ToolMetadataPolicyValidationResult {
        ok: validation.ok(),
        diagnostics: validation.diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolProfileValidationInput {
    pub profile: LocalToolProfilePolicy,
    pub catalog: LocalToolProfileCatalogPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolProfileCatalogPolicy {
    #[serde(default)]
    pub toolsets: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolProfilePolicy {
    pub id: String,
    pub enabled: bool,
    pub system: bool,
    pub read_only: bool,
    #[serde(default)]
    pub toolsets: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolProfileValidationIssue {
    pub reason_code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalToolProfileValidationResult {
    pub ok: bool,
    pub issues: Vec<LocalToolProfileValidationIssue>,
}

pub fn validate_local_tool_profile_policy(
    input: &LocalToolProfileValidationInput,
) -> LocalToolProfileValidationResult {
    let mut issues = Vec::new();
    validate_local_profile_id(&input.profile.id, &mut issues);
    if input.profile.read_only && !input.profile.system {
        issues.push(LocalToolProfileValidationIssue {
            reason_code: "local_tool_profile_read_only_requires_system".to_string(),
            path: "readOnly".to_string(),
            message: format!(
                "local tool profile {} cannot be read-only unless it is a system profile",
                input.profile.id
            ),
        });
    }

    let known_toolsets = input
        .catalog
        .toolsets
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let known_tools = input
        .catalog
        .tools
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    validate_local_profile_refs(
        &input.profile.id,
        "toolsets",
        &input.profile.toolsets,
        &known_toolsets,
        "local_tool_profile_unknown_toolset",
        "built-in toolset",
        &mut issues,
    );
    validate_local_profile_refs(
        &input.profile.id,
        "tools",
        &input.profile.tools,
        &known_tools,
        "local_tool_profile_unknown_tool",
        "built-in tool",
        &mut issues,
    );

    LocalToolProfileValidationResult {
        ok: issues.is_empty(),
        issues,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalMemoryToolMode {
    Off,
    Metadata,
    Candidate,
    Manual,
    Permissive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMemoryDependencyPolicy {
    pub configured: bool,
    pub client_available: bool,
    pub mode: ExternalMemoryToolMode,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAvailabilityPlanInput {
    #[serde(default)]
    pub selected_tools: Vec<String>,
    pub den_memory: ExternalMemoryDependencyPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAvailabilityOmission {
    pub tool_name: String,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAvailabilityPlan {
    pub selected_tools: Vec<String>,
    pub omitted_tools: Vec<ToolAvailabilityOmission>,
    pub diagnostics: Vec<ToolAvailabilityOmission>,
}

pub fn plan_tool_availability(input: &ToolAvailabilityPlanInput) -> ToolAvailabilityPlan {
    let mut selected_tools = Vec::new();
    let mut omitted_tools = Vec::new();
    for tool_name in &input.selected_tools {
        if let Some(omission) = external_memory_tool_omission(tool_name, &input.den_memory) {
            omitted_tools.push(omission);
        } else {
            selected_tools.push(tool_name.clone());
        }
    }
    ToolAvailabilityPlan {
        selected_tools,
        diagnostics: omitted_tools.clone(),
        omitted_tools,
    }
}

fn external_memory_tool_omission(
    tool_name: &str,
    policy: &ExternalMemoryDependencyPolicy,
) -> Option<ToolAvailabilityOmission> {
    if !is_external_memory_tool(tool_name) {
        return None;
    }
    if policy.mode == ExternalMemoryToolMode::Off {
        return Some(omission(
            tool_name,
            "memory_policy_off",
            "external memory policy is off",
        ));
    }
    if !policy.configured {
        return Some(omission(
            tool_name,
            "memory_external_dependency_missing",
            "external memory is not configured",
        ));
    }
    if !policy.client_available {
        return Some(omission(
            tool_name,
            "memory_external_dependency_unavailable",
            policy
                .last_error
                .as_deref()
                .unwrap_or("external memory client is unavailable"),
        ));
    }
    if policy.mode == ExternalMemoryToolMode::Metadata
        && matches!(tool_name, "memory_store" | "memory_propose")
    {
        return Some(omission(
            tool_name,
            "memory_writes_disabled_metadata_mode",
            "metadata-only external memory mode exposes read tools only",
        ));
    }
    None
}

fn is_external_memory_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "memory_recall" | "memory_read" | "memory_search" | "memory_store" | "memory_propose"
    )
}

fn omission(
    tool_name: &str,
    reason_code: impl Into<String>,
    message: impl Into<String>,
) -> ToolAvailabilityOmission {
    ToolAvailabilityOmission {
        tool_name: tool_name.to_string(),
        reason_code: reason_code.into(),
        message: message.into(),
    }
}

struct ToolMetadataValidator<'a> {
    entries: &'a [ToolMetadata],
    diagnostics: Vec<ToolMetadataDiagnostic>,
}

impl<'a> ToolMetadataValidator<'a> {
    fn new(entries: &'a [ToolMetadata]) -> Self {
        Self {
            entries,
            diagnostics: Vec::new(),
        }
    }

    fn validate(&mut self) {
        let mut canonical_names: HashMap<&str, usize> = HashMap::new();
        let mut aliases: HashMap<&str, usize> = HashMap::new();

        for (index, entry) in self.entries.iter().enumerate() {
            self.validate_entry(index, entry);

            if let Some(existing) = canonical_names.insert(entry.name.as_str(), index) {
                self.error(
                    "duplicate_name",
                    Some(entry.name.as_str()),
                    Some(self.entries[existing].name.as_str()),
                    format!("tools[{index}].name"),
                    format!("duplicate canonical tool name {}", entry.name),
                );
            }

            let mut local_aliases = HashSet::new();
            for alias in &entry.aliases {
                if !local_aliases.insert(alias.as_str()) {
                    self.error(
                        "duplicate_alias",
                        Some(entry.name.as_str()),
                        Some(entry.name.as_str()),
                        format!("tools[{index}].aliases"),
                        format!("alias {alias} is repeated on {}", entry.name),
                    );
                }
                if let Some(existing) = aliases.insert(alias.as_str(), index) {
                    self.error(
                        "duplicate_alias",
                        Some(entry.name.as_str()),
                        Some(self.entries[existing].name.as_str()),
                        format!("tools[{index}].aliases"),
                        format!("alias {alias} is used by multiple tools"),
                    );
                }
            }
        }

        self.validate_alias_name_collisions(&canonical_names);
        self.validate_capability_collisions();
        self.validate_deprecations();
    }

    fn validate_entry(&mut self, index: usize, entry: &ToolMetadata) {
        if !valid_tool_name(&entry.name) {
            self.error(
                "invalid_name",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].name"),
                format!("tool name {} must be lower snake case", entry.name),
            );
        }
        if entry.description.trim().is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].description"),
                "tool description is required",
            );
        }
        if entry.toolsets.is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].toolsets"),
                "at least one toolset is required",
            );
        }
        if entry.safety.is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].safety"),
                "at least one safety flag is required",
            );
        }
        if entry.surfaces.is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].surfaces"),
                "at least one surface is required",
            );
        }
        if entry.output_shape.trim().is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].outputShape"),
                "output shape is required",
            );
        }
        if entry.version.trim().is_empty() {
            self.error(
                "missing_metadata",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].version"),
                "version is required",
            );
        }
        if !entry.output_shape.trim().is_empty() && !valid_output_shape(&entry.output_shape) {
            self.error(
                "invalid_output_shape",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].outputShape"),
                format!(
                    "output shape {} must be dot-delimited lower ids ending in vN",
                    entry.output_shape
                ),
            );
        }
        if !entry.version.trim().is_empty() && !valid_semver(&entry.version) {
            self.error(
                "invalid_version",
                Some(entry.name.as_str()),
                None,
                format!("tools[{index}].version"),
                format!(
                    "version {} must be semver-like MAJOR.MINOR.PATCH",
                    entry.version
                ),
            );
        }

        for (alias_index, alias) in entry.aliases.iter().enumerate() {
            if !valid_tool_name(alias) {
                self.error(
                    "invalid_alias",
                    Some(entry.name.as_str()),
                    None,
                    format!("tools[{index}].aliases[{alias_index}]"),
                    format!("alias {alias} must be lower snake case"),
                );
            }
            if alias == &entry.name {
                self.error(
                    "alias_collides_with_name",
                    Some(entry.name.as_str()),
                    Some(entry.name.as_str()),
                    format!("tools[{index}].aliases[{alias_index}]"),
                    format!("alias {alias} duplicates its canonical tool name"),
                );
            }
        }
        self.validate_unique_named_values(index, entry, "toolsets", &entry.toolsets);
        self.validate_unique_enum_values(index, entry, "surfaces", &entry.surfaces);
        self.validate_unique_enum_values(index, entry, "safety", &entry.safety);
        for (toolset_index, toolset) in entry.toolsets.iter().enumerate() {
            if !valid_toolset_name(toolset) {
                self.error(
                    "invalid_toolset",
                    Some(entry.name.as_str()),
                    None,
                    format!("tools[{index}].toolsets[{toolset_index}]"),
                    format!(
                        "toolset {toolset} must be lower snake case or an mcp: dynamic toolset id"
                    ),
                );
            }
        }
    }

    fn validate_alias_name_collisions(&mut self, canonical_names: &HashMap<&str, usize>) {
        for (index, entry) in self.entries.iter().enumerate() {
            for (alias_index, alias) in entry.aliases.iter().enumerate() {
                let Some(existing) = canonical_names.get(alias.as_str()) else {
                    continue;
                };
                let canonical = &self.entries[*existing];
                if canonical.name == entry.name {
                    continue;
                }
                self.error(
                    "alias_collides_with_name",
                    Some(entry.name.as_str()),
                    Some(canonical.name.as_str()),
                    format!("tools[{index}].aliases[{alias_index}]"),
                    format!(
                        "alias {alias} collides with canonical tool {}",
                        canonical.name
                    ),
                );
            }
        }
    }

    fn validate_capability_collisions(&mut self) {
        let mut capability_owners: HashMap<(&ToolCategory, &str), usize> = HashMap::new();
        for (index, entry) in self.entries.iter().enumerate() {
            if entry.deprecated.is_some() {
                continue;
            }
            let key = (&entry.category, entry.output_shape.as_str());
            let Some(existing) = capability_owners.insert(key, index) else {
                continue;
            };
            let other = &self.entries[existing];
            if has_coexistence_note(entry) || has_coexistence_note(other) {
                continue;
            }
            self.error(
                "capability_collision",
                Some(entry.name.as_str()),
                Some(other.name.as_str()),
                format!("tools[{index}].outputShape"),
                format!(
                    "{} and {} both claim {:?}:{}",
                    entry.name, other.name, entry.category, entry.output_shape
                ),
            );
        }
    }

    fn validate_deprecations(&mut self) {
        let names: HashSet<&str> = self
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        for (index, entry) in self.entries.iter().enumerate() {
            let replacement = entry
                .replacement
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    entry
                        .deprecated
                        .as_ref()
                        .and_then(|deprecated| deprecated.replacement.as_deref())
                        .filter(|value| !value.trim().is_empty())
                });
            if entry.deprecated.is_some() && replacement.is_none() {
                self.error(
                    "deprecated_without_replacement",
                    Some(entry.name.as_str()),
                    None,
                    format!("tools[{index}].deprecated"),
                    format!("deprecated tool {} needs a replacement", entry.name),
                );
            }
            if entry.deprecated.is_none() && replacement.is_some() {
                self.error(
                    "replacement_without_deprecation",
                    Some(entry.name.as_str()),
                    replacement,
                    format!("tools[{index}].replacement"),
                    format!(
                        "replacement is only valid for deprecated tool {}",
                        entry.name
                    ),
                );
            }
            if let Some(deprecated) = &entry.deprecated {
                if deprecated.reason.trim().is_empty() {
                    self.error(
                        "bad_deprecation",
                        Some(entry.name.as_str()),
                        None,
                        format!("tools[{index}].deprecated.reason"),
                        "deprecation reason is required",
                    );
                }
                if !valid_semver(&deprecated.since) {
                    self.error(
                        "bad_deprecation",
                        Some(entry.name.as_str()),
                        None,
                        format!("tools[{index}].deprecated.since"),
                        format!("deprecation since {} must be semver-like", deprecated.since),
                    );
                }
                if deprecated
                    .sunset
                    .as_deref()
                    .is_some_and(|sunset| !valid_semver(sunset))
                {
                    self.error(
                        "bad_deprecation",
                        Some(entry.name.as_str()),
                        None,
                        format!("tools[{index}].deprecated.sunset"),
                        "deprecation sunset must be semver-like when present",
                    );
                }
            }
            if let Some(replacement) = replacement {
                if !valid_tool_name(replacement) {
                    self.error(
                        "invalid_replacement",
                        Some(entry.name.as_str()),
                        None,
                        format!("tools[{index}].replacement"),
                        format!("replacement {replacement} must be lower snake case"),
                    );
                }
                if replacement == entry.name {
                    self.error(
                        "deprecated_replacement_self_reference",
                        Some(entry.name.as_str()),
                        Some(entry.name.as_str()),
                        format!("tools[{index}].replacement"),
                        format!("deprecated tool {} cannot replace itself", entry.name),
                    );
                }
                if !names.contains(replacement) {
                    self.error(
                        "missing_replacement_tool",
                        Some(entry.name.as_str()),
                        Some(replacement),
                        format!("tools[{index}].replacement"),
                        format!("replacement tool {replacement} is not registered"),
                    );
                }
            }
        }
    }

    fn validate_unique_named_values(
        &mut self,
        index: usize,
        entry: &ToolMetadata,
        field: &str,
        values: &[String],
    ) {
        let mut seen = HashSet::new();
        for (value_index, value) in values.iter().enumerate() {
            if !seen.insert(value.as_str()) {
                self.error(
                    "duplicate_metadata_value",
                    Some(entry.name.as_str()),
                    Some(entry.name.as_str()),
                    format!("tools[{index}].{field}[{value_index}]"),
                    format!("{field} value {value} is repeated on {}", entry.name),
                );
            }
        }
    }

    fn validate_unique_enum_values<T>(
        &mut self,
        index: usize,
        entry: &ToolMetadata,
        field: &str,
        values: &[T],
    ) where
        T: std::fmt::Debug + Eq + std::hash::Hash,
    {
        let mut seen = HashSet::new();
        for (value_index, value) in values.iter().enumerate() {
            if !seen.insert(value) {
                self.error(
                    "duplicate_metadata_value",
                    Some(entry.name.as_str()),
                    Some(entry.name.as_str()),
                    format!("tools[{index}].{field}[{value_index}]"),
                    format!("{field} value {value:?} is repeated on {}", entry.name),
                );
            }
        }
    }

    fn error(
        &mut self,
        code: impl Into<String>,
        tool_name: Option<&str>,
        other_tool_name: Option<&str>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(ToolMetadataDiagnostic::error(
            code,
            tool_name,
            other_tool_name,
            path,
            message,
        ));
    }
}

fn has_coexistence_note(entry: &ToolMetadata) -> bool {
    entry
        .coexistence_note
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
        || entry
            .collision_notes
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
}

fn valid_tool_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    let mut previous_underscore = false;
    for c in chars {
        if c == '_' {
            if previous_underscore {
                return false;
            }
            previous_underscore = true;
            continue;
        }
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() {
            return false;
        }
        previous_underscore = false;
    }
    !previous_underscore
}

fn valid_toolset_name(value: &str) -> bool {
    if let Some(dynamic_id) = value.strip_prefix("mcp:") {
        return valid_dynamic_mcp_toolset_id(dynamic_id);
    }
    valid_tool_name(value)
}

fn valid_dynamic_mcp_toolset_id(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    chars.all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.'))
}

fn validate_local_profile_id(id: &str, issues: &mut Vec<LocalToolProfileValidationIssue>) {
    if id.trim().is_empty() {
        issues.push(LocalToolProfileValidationIssue {
            reason_code: "local_tool_profile_id_required".to_string(),
            path: "id".to_string(),
            message: "local tool profile id is required".to_string(),
        });
        return;
    }
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return;
    };
    if !first.is_ascii_alphanumeric()
        || id.len() > 80
        || !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-'))
    {
        issues.push(LocalToolProfileValidationIssue {
            reason_code: "local_tool_profile_invalid_id".to_string(),
            path: "id".to_string(),
            message:
                "local tool profile id must start with a letter or number and contain only letters, numbers, underscore, dot, colon, or hyphen"
                    .to_string(),
        });
    }
}

fn validate_local_profile_refs(
    profile_id: &str,
    field: &str,
    refs: &[String],
    known_refs: &HashSet<&str>,
    unknown_reason_code: &str,
    label: &str,
    issues: &mut Vec<LocalToolProfileValidationIssue>,
) {
    let mut seen = HashSet::new();
    for (index, item) in refs.iter().enumerate() {
        let path = format!("{field}[{index}]");
        if !seen.insert(item.as_str()) {
            issues.push(LocalToolProfileValidationIssue {
                reason_code: format!("local_tool_profile_duplicate_{field}"),
                path,
                message: format!("local tool profile {profile_id} repeats {label} {item}"),
            });
            continue;
        }
        if field == "toolsets" && item.starts_with("mcp:") {
            issues.push(LocalToolProfileValidationIssue {
                reason_code: "local_tool_profile_rejects_mcp_toolset".to_string(),
                path,
                message: format!(
                    "local tool profile {profile_id} cannot reference dynamic MCP toolset {item}"
                ),
            });
            continue;
        }
        if !known_refs.contains(item.as_str()) {
            issues.push(LocalToolProfileValidationIssue {
                reason_code: unknown_reason_code.to_string(),
                path,
                message: format!(
                    "local tool profile {profile_id} references unknown {label} {item}"
                ),
            });
        }
    }
}

fn valid_output_shape(value: &str) -> bool {
    let mut parts = value.split('.').collect::<Vec<_>>();
    if parts.len() < 3 {
        return false;
    }
    let Some(version) = parts.pop() else {
        return false;
    };
    if !version
        .strip_prefix('v')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
    {
        return false;
    }
    parts.into_iter().all(valid_tool_name)
}

fn valid_semver(value: &str) -> bool {
    let parts = value.split('.').collect::<Vec<_>>();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|ch| ch.is_ascii_digit())
                && (part == &"0" || !part.starts_with('0'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ToolRegistryMetadataArtifact {
        schema_version: u32,
        catalog_id: String,
        tools: Vec<ToolMetadata>,
    }

    #[test]
    fn validates_shared_default_tool_registry_artifact() {
        let artifact: ToolRegistryMetadataArtifact = serde_json::from_str(include_str!(
            "../../../../fixtures/tool-registry/default-tool-registry-metadata.json"
        ))
        .expect("shared tool registry metadata artifact should deserialize");

        assert_eq!(artifact.schema_version, 1);
        assert_eq!(artifact.catalog_id, "default-local-tools");
        assert_eq!(artifact.tools.len(), 57);

        let result = validate_tool_metadata_list(&artifact.tools);

        assert!(result.ok(), "{:?}", result.diagnostics);
    }

    #[test]
    fn validates_portable_metadata_without_executor_binding() {
        let result = validate_tool_metadata_list(&[
            tool("read_file", ToolCategory::Local, "local.file_text.v1"),
            tool("web_extract", ToolCategory::Web, "web.extract_result.v1"),
        ]);

        assert!(result.ok(), "{:?}", result.diagnostics);
    }

    #[test]
    fn reports_duplicate_name() {
        let result = validate_tool_metadata_list(&[
            tool("read_file", ToolCategory::Local, "local.file_text.v1"),
            tool("read_file", ToolCategory::Git, "git.status_result.v1"),
        ]);

        assert_codes(&result, &["duplicate_name"]);
    }

    #[test]
    fn validates_tool_metadata_policy_with_ok_flag() {
        let result = validate_tool_metadata_policy(&ToolMetadataPolicyValidationInput {
            tools: vec![
                tool("read_file", ToolCategory::Local, "local.file_text.v1"),
                tool(
                    "read_file",
                    ToolCategory::Mcp,
                    "mcp.den.read_file.result.v1",
                ),
            ],
        });

        assert!(!result.ok);
        assert_eq!(result.diagnostics[0].code, "duplicate_name");
    }

    #[test]
    fn allows_dynamic_mcp_toolset_ids_in_portable_metadata() {
        let mut entry = tool("den_search", ToolCategory::Mcp, "mcp.den.search.result.v1");
        entry.toolsets = vec!["mcp:prime-mcp".to_string()];
        entry.surfaces = vec![ToolSurface::Brain, ToolSurface::Mcp];
        entry.safety = vec![ToolSafetyFlag::NetworkAccess];

        let result = validate_tool_metadata_policy(&ToolMetadataPolicyValidationInput {
            tools: vec![entry],
        });

        assert!(result.ok, "{:?}", result.diagnostics);
    }

    #[test]
    fn reports_duplicate_alias() {
        let mut a = tool("read_file", ToolCategory::Local, "local.file_text.v1");
        a.aliases = vec!["file_read".to_string()];
        let mut b = tool(
            "search_files",
            ToolCategory::Local,
            "local.file_search_result.v1",
        );
        b.aliases = vec!["file_read".to_string()];

        let result = validate_tool_metadata_list(&[a, b]);

        assert_codes(&result, &["duplicate_alias"]);
    }

    #[test]
    fn reports_alias_name_collision() {
        let a = tool("read_file", ToolCategory::Local, "local.file_text.v1");
        let mut b = tool(
            "search_files",
            ToolCategory::Local,
            "local.file_search_result.v1",
        );
        b.aliases = vec!["read_file".to_string()];

        let result = validate_tool_metadata_list(&[a, b]);

        assert_codes(&result, &["alias_collides_with_name"]);
    }

    #[test]
    fn reports_capability_collision_without_coexistence_note() {
        let result = validate_tool_metadata_list(&[
            tool("read_file", ToolCategory::Local, "local.file_text.v1"),
            tool("cat_file", ToolCategory::Local, "local.file_text.v1"),
        ]);

        assert_codes(&result, &["capability_collision"]);
    }

    #[test]
    fn allows_capability_collision_with_valid_coexistence_note() {
        let a = tool("read_file", ToolCategory::Local, "local.file_text.v1");
        let mut b = tool("preview_file", ToolCategory::Local, "local.file_text.v1");
        b.coexistence_note = Some("preview_file returns truncated display text".to_string());

        let result = validate_tool_metadata_list(&[a, b]);

        assert!(result.ok(), "{:?}", result.diagnostics);
    }

    #[test]
    fn reports_deprecated_without_replacement() {
        let mut old = tool(
            "old_memory_recall",
            ToolCategory::Memory,
            "memory.recall.v1",
        );
        old.deprecated = Some(ToolDeprecation {
            reason: "renamed for clarity".to_string(),
            since: "0.2.0".to_string(),
            replacement: None,
            sunset: None,
        });

        let result = validate_tool_metadata_list(&[old]);

        assert_codes(&result, &["deprecated_without_replacement"]);
    }

    #[test]
    fn accepts_deprecated_tool_with_registered_replacement() {
        let replacement = tool(
            "den_memory_recall",
            ToolCategory::Memory,
            "memory.recall.v2",
        );
        let mut old = tool(
            "old_memory_recall",
            ToolCategory::Memory,
            "memory.recall.v1",
        );
        old.deprecated = Some(ToolDeprecation {
            reason: "renamed for clarity".to_string(),
            since: "0.2.0".to_string(),
            replacement: Some("den_memory_recall".to_string()),
            sunset: None,
        });

        let result = validate_tool_metadata_list(&[replacement, old]);

        assert!(result.ok(), "{:?}", result.diagnostics);
    }

    #[test]
    fn reports_invalid_name_and_missing_metadata() {
        let mut bad = tool("BadTool", ToolCategory::Diagnostics, "diagnostics.bad.v1");
        bad.description.clear();
        bad.toolsets.clear();
        bad.surfaces.clear();
        bad.safety.clear();
        bad.output_shape.clear();
        bad.version.clear();

        let result = validate_tool_metadata_list(&[bad]);

        assert_codes(
            &result,
            &[
                "invalid_name",
                "missing_metadata",
                "missing_metadata",
                "missing_metadata",
                "missing_metadata",
                "missing_metadata",
                "missing_metadata",
            ],
        );
    }

    #[test]
    fn reports_invalid_portable_ids_and_versions() {
        let mut bad = tool("read_file", ToolCategory::Local, "Local.FileText.v1");
        bad.toolsets = vec!["local_code_read".to_string(), "BadToolset".to_string()];
        bad.version = "01.0".to_string();

        let result = validate_tool_metadata_list(&[bad]);

        assert_codes(
            &result,
            &["invalid_output_shape", "invalid_version", "invalid_toolset"],
        );
    }

    #[test]
    fn reports_duplicate_portable_metadata_values() {
        let mut bad = tool("read_file", ToolCategory::Local, "local.file_text.v1");
        bad.toolsets = vec!["local_code_read".to_string(), "local_code_read".to_string()];
        bad.surfaces = vec![ToolSurface::Brain, ToolSurface::Brain];
        bad.safety = vec![ToolSafetyFlag::ReadOnly, ToolSafetyFlag::ReadOnly];

        let result = validate_tool_metadata_list(&[bad]);

        assert_codes(
            &result,
            &[
                "duplicate_metadata_value",
                "duplicate_metadata_value",
                "duplicate_metadata_value",
            ],
        );
    }

    #[test]
    fn reports_bad_deprecation_shape() {
        let replacement = tool("memory_recall", ToolCategory::Memory, "memory.recall.v2");
        let mut old = tool(
            "old_memory_recall",
            ToolCategory::Memory,
            "memory.recall.v1",
        );
        old.deprecated = Some(ToolDeprecation {
            reason: String::new(),
            since: "next".to_string(),
            replacement: Some("memory_recall".to_string()),
            sunset: Some("soon".to_string()),
        });

        let result = validate_tool_metadata_list(&[replacement, old]);

        assert_codes(
            &result,
            &["bad_deprecation", "bad_deprecation", "bad_deprecation"],
        );
    }

    #[test]
    fn reports_replacement_without_deprecation() {
        let replacement = tool("memory_recall", ToolCategory::Memory, "memory.recall.v2");
        let mut current = tool(
            "old_memory_recall",
            ToolCategory::Memory,
            "memory.recall.v1",
        );
        current.replacement = Some("memory_recall".to_string());

        let result = validate_tool_metadata_list(&[replacement, current]);

        assert_codes(&result, &["replacement_without_deprecation"]);
    }

    #[test]
    fn rejects_unsupported_enum_values_during_deserialization() {
        let raw = serde_json::json!({
            "name": "read_file",
            "description": "read file",
            "category": "wormhole",
            "toolsets": ["local_code_read"],
            "surfaces": ["brain"],
            "safety": ["read_only"],
            "output_shape": "local.file_text.v1",
            "version": "1.0.0"
        });

        let error = serde_json::from_value::<ToolMetadata>(raw).unwrap_err();

        assert!(
            error.to_string().contains("unknown variant"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn validates_local_tool_profile_policy() {
        let result = validate_local_tool_profile_policy(&LocalToolProfileValidationInput {
            profile: LocalToolProfilePolicy {
                id: "code_read".to_string(),
                enabled: true,
                system: true,
                read_only: true,
                toolsets: vec!["local_code_read".to_string()],
                tools: vec!["read_file".to_string()],
            },
            catalog: local_profile_catalog(),
        });

        assert!(result.ok, "{:?}", result.issues);
    }

    #[test]
    fn rejects_local_tool_profile_unknown_refs_and_dynamic_mcp() {
        let result = validate_local_tool_profile_policy(&LocalToolProfileValidationInput {
            profile: LocalToolProfilePolicy {
                id: "custom".to_string(),
                enabled: true,
                system: false,
                read_only: false,
                toolsets: vec![
                    "mcp:planner".to_string(),
                    "missing_toolset".to_string(),
                    "local_code_read".to_string(),
                    "local_code_read".to_string(),
                ],
                tools: vec![
                    "missing_tool".to_string(),
                    "read_file".to_string(),
                    "read_file".to_string(),
                ],
            },
            catalog: local_profile_catalog(),
        });

        assert_local_profile_codes(
            &result,
            &[
                "local_tool_profile_rejects_mcp_toolset",
                "local_tool_profile_unknown_toolset",
                "local_tool_profile_duplicate_toolsets",
                "local_tool_profile_unknown_tool",
                "local_tool_profile_duplicate_tools",
            ],
        );
    }

    #[test]
    fn rejects_local_tool_profile_bad_id_and_read_only_custom_profile() {
        let result = validate_local_tool_profile_policy(&LocalToolProfileValidationInput {
            profile: LocalToolProfilePolicy {
                id: "-bad".to_string(),
                enabled: true,
                system: false,
                read_only: true,
                toolsets: vec![],
                tools: vec![],
            },
            catalog: local_profile_catalog(),
        });

        assert_local_profile_codes(
            &result,
            &[
                "local_tool_profile_invalid_id",
                "local_tool_profile_read_only_requires_system",
            ],
        );
    }

    #[test]
    fn tool_availability_omits_external_memory_when_client_unavailable() {
        let plan = plan_tool_availability(&ToolAvailabilityPlanInput {
            selected_tools: vec![
                "memory_search".to_string(),
                "memory_store".to_string(),
                "den_project_list_tasks".to_string(),
            ],
            den_memory: ExternalMemoryDependencyPolicy {
                configured: true,
                client_available: false,
                mode: ExternalMemoryToolMode::Candidate,
                last_error: Some("memory endpoint timed out".to_string()),
            },
        });

        assert_eq!(plan.selected_tools, vec!["den_project_list_tasks"]);
        assert_eq!(plan.omitted_tools.len(), 2);
        assert!(plan
            .omitted_tools
            .iter()
            .all(|omission| { omission.reason_code == "memory_external_dependency_unavailable" }));
    }

    #[test]
    fn tool_availability_metadata_mode_keeps_reads_and_omits_writes() {
        let plan = plan_tool_availability(&ToolAvailabilityPlanInput {
            selected_tools: vec![
                "memory_recall".to_string(),
                "memory_read".to_string(),
                "memory_search".to_string(),
                "memory_store".to_string(),
                "memory_propose".to_string(),
                "mcp_den_documents_read".to_string(),
            ],
            den_memory: ExternalMemoryDependencyPolicy {
                configured: true,
                client_available: true,
                mode: ExternalMemoryToolMode::Metadata,
                last_error: None,
            },
        });

        assert_eq!(
            plan.selected_tools,
            vec![
                "memory_recall",
                "memory_read",
                "memory_search",
                "mcp_den_documents_read"
            ]
        );
        assert_eq!(
            plan.omitted_tools
                .iter()
                .map(|omission| omission.tool_name.as_str())
                .collect::<Vec<_>>(),
            vec!["memory_store", "memory_propose"]
        );
    }

    fn tool(name: &str, category: ToolCategory, output_shape: &str) -> ToolMetadata {
        ToolMetadata {
            name: name.to_string(),
            description: format!("{name} description"),
            aliases: Vec::new(),
            category,
            toolsets: vec!["default".to_string()],
            surfaces: vec![ToolSurface::Brain],
            safety: vec![ToolSafetyFlag::ReadOnly],
            output_shape: output_shape.to_string(),
            version: "1.0.0".to_string(),
            deprecated: None,
            replacement: None,
            coexistence_note: None,
            collision_notes: None,
        }
    }

    fn assert_codes(result: &ToolMetadataValidationResult, expected: &[&str]) {
        let mut actual: Vec<&str> = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect();
        for code in expected {
            let Some(index) = actual.iter().position(|actual| actual == code) else {
                panic!("missing diagnostic code {code}; actual={actual:?}");
            };
            actual.remove(index);
        }
    }

    fn assert_local_profile_codes(result: &LocalToolProfileValidationResult, expected: &[&str]) {
        let actual = result
            .issues
            .iter()
            .map(|issue| issue.reason_code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
    }

    fn local_profile_catalog() -> LocalToolProfileCatalogPolicy {
        LocalToolProfileCatalogPolicy {
            toolsets: vec![
                "local_code_read".to_string(),
                "local_code_write".to_string(),
            ],
            tools: vec!["read_file".to_string(), "write_file".to_string()],
        }
    }
}
