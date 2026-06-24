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
    Skills,
    Mcp,
    Delegation,
    Planning,
    Diagnostics,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSurface {
    Brain,
    Mcp,
    Admin,
    Tui,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolSafetyFlag {
    ReadOnly,
    WritesFiles,
    ExecutesProcess,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            ],
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
}
