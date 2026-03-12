use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct PnpmWorkspaceYaml {
    #[serde(default)]
    pub packages: Vec<String>,

    #[serde(default)]
    pub catalog: IndexMap<String, String>,

    #[serde(default)]
    pub catalogs: IndexMap<String, IndexMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogEntry {
    /// None = default catalog, Some(name) = named catalog
    pub catalog_name: Option<String>,
    pub dependency_name: String,
}

#[derive(Debug)]
pub struct WorkspaceCatalogs {
    pub default: IndexMap<String, String>,
    pub named: IndexMap<String, IndexMap<String, String>>,
}

impl WorkspaceCatalogs {
    pub fn has_default_entry(&self, dep_name: &str) -> bool {
        self.default.contains_key(dep_name)
    }

    pub fn has_named_entry(&self, catalog_name: &str, dep_name: &str) -> bool {
        self.named
            .get(catalog_name)
            .is_some_and(|entries| entries.contains_key(dep_name))
    }

    pub fn has_catalog(&self, catalog_name: &str) -> bool {
        self.named.contains_key(catalog_name)
    }

    pub fn all_entries(&self) -> HashSet<CatalogEntry> {
        let mut entries = HashSet::new();
        for dep_name in self.default.keys() {
            entries.insert(CatalogEntry {
                catalog_name: None,
                dependency_name: dep_name.clone(),
            });
        }
        for (catalog_name, deps) in &self.named {
            for dep_name in deps.keys() {
                entries.insert(CatalogEntry {
                    catalog_name: Some(catalog_name.clone()),
                    dependency_name: dep_name.clone(),
                });
            }
        }
        entries
    }

    pub fn get_version(&self, entry: &CatalogEntry) -> Option<&str> {
        match &entry.catalog_name {
            None => self.default.get(&entry.dependency_name).map(|s| s.as_str()),
            Some(name) => self
                .named
                .get(name)
                .and_then(|deps| deps.get(&entry.dependency_name))
                .map(|s| s.as_str()),
        }
    }

    /// Check if a dependency name exists in any catalog (default or named).
    /// Returns a list of catalog names where it's found (None = default).
    pub fn find_dependency(&self, dep_name: &str) -> Vec<Option<String>> {
        let mut found = Vec::new();
        if self.default.contains_key(dep_name) {
            found.push(None);
        }
        for (catalog_name, deps) in &self.named {
            if deps.contains_key(dep_name) {
                found.push(Some(catalog_name.clone()));
            }
        }
        found
    }
}

pub fn parse_workspace(root: &Path) -> Result<(PnpmWorkspaceYaml, WorkspaceCatalogs)> {
    let yaml_path = root.join("pnpm-workspace.yaml");
    let content = std::fs::read_to_string(&yaml_path)
        .with_context(|| format!("Failed to read {}", yaml_path.display()))?;

    let workspace: PnpmWorkspaceYaml =
        serde_yaml::from_str(&content).context("Failed to parse pnpm-workspace.yaml")?;

    let catalogs = WorkspaceCatalogs {
        default: workspace.catalog.clone(),
        named: workspace.catalogs.clone(),
    };

    Ok((workspace, catalogs))
}

/// Extract the YAML key from a line like `  react: "^18.2.0"` or `  "@types/react": "^18.0.0"`.
/// Returns `None` if the line doesn't look like a key-value pair at the expected indent.
fn extract_yaml_key(line: &str, expected_indent: usize) -> Option<&str> {
    // Check correct indentation
    let spaces = line.len() - line.trim_start().len();
    if spaces != expected_indent {
        return None;
    }

    let trimmed = line.trim_start();

    // Skip blank lines, comments, and section headers (lines ending with just `:` or `: `)
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    // Handle quoted keys: "key": value or 'key': value
    if trimmed.starts_with('"') {
        let end = trimmed[1..].find('"')?;
        let key = &trimmed[1..end + 1];
        // Verify followed by `:`
        if trimmed.as_bytes().get(end + 2) == Some(&b':') {
            return Some(key);
        }
        return None;
    }
    if trimmed.starts_with('\'') {
        let end = trimmed[1..].find('\'')?;
        let key = &trimmed[1..end + 1];
        if trimmed.as_bytes().get(end + 2) == Some(&b':') {
            return Some(key);
        }
        return None;
    }

    // Unquoted key: everything before the first `:`
    let colon_pos = trimmed.find(':')?;
    if colon_pos == 0 {
        return None;
    }
    Some(&trimmed[..colon_pos])
}

#[derive(Debug, PartialEq)]
enum YamlSection {
    Other,
    DefaultCatalog,
    CatalogsHeader,
    NamedCatalog(String),
}

/// Remove unused catalog entries from `pnpm-workspace.yaml` using line-based editing.
/// Returns the number of entries removed.
pub fn remove_catalog_entries(root: &Path, entries: &[CatalogEntry]) -> Result<usize> {
    let yaml_path = root.join("pnpm-workspace.yaml");
    let content = std::fs::read_to_string(&yaml_path)
        .with_context(|| format!("Failed to read {}", yaml_path.display()))?;

    let line_ending = if content.contains("\r\n") { "\r\n" } else { "\n" };
    let lines: Vec<&str> = content.split('\n').collect();

    // Build a set for quick lookup
    let to_remove: HashSet<&CatalogEntry> = entries.iter().collect();

    // First pass: identify section context and mark lines for removal
    let mut remove_lines: HashSet<usize> = HashSet::new();
    let mut section = YamlSection::Other;
    let mut removed_count = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end_matches('\r');

        // Detect top-level section transitions (zero indent, non-blank)
        if !trimmed.is_empty() && !trimmed.starts_with(' ') && !trimmed.starts_with('#') {
            if trimmed == "catalog:" || trimmed.starts_with("catalog:") && trimmed[8..].trim().is_empty() {
                section = YamlSection::DefaultCatalog;
                continue;
            } else if trimmed == "catalogs:" || trimmed.starts_with("catalogs:") && trimmed[9..].trim().is_empty() {
                section = YamlSection::CatalogsHeader;
                continue;
            } else {
                section = YamlSection::Other;
                continue;
            }
        }

        match &section {
            YamlSection::DefaultCatalog => {
                if let Some(key) = extract_yaml_key(trimmed, 2) {
                    let entry = CatalogEntry {
                        catalog_name: None,
                        dependency_name: key.to_string(),
                    };
                    if to_remove.contains(&entry) {
                        remove_lines.insert(i);
                        removed_count += 1;
                    }
                }
            }
            YamlSection::CatalogsHeader => {
                // Check for named catalog header at indent 2, e.g. `  react16:`
                if let Some(key) = extract_yaml_key(trimmed, 2) {
                    // This is a named catalog header — but extract_yaml_key looks for `key: value`
                    // Named catalog headers are `name:` followed by children. Check if it's a section header.
                    section = YamlSection::NamedCatalog(key.to_string());
                }
            }
            YamlSection::NamedCatalog(catalog_name) => {
                // Entries at indent 4
                if let Some(key) = extract_yaml_key(trimmed, 4) {
                    let entry = CatalogEntry {
                        catalog_name: Some(catalog_name.clone()),
                        dependency_name: key.to_string(),
                    };
                    if to_remove.contains(&entry) {
                        remove_lines.insert(i);
                        removed_count += 1;
                    }
                } else if let Some(_key) = extract_yaml_key(trimmed, 2) {
                    // New named catalog section at indent 2
                    section = YamlSection::NamedCatalog(_key.to_string());
                }
            }
            YamlSection::Other => {}
        }
    }

    if removed_count == 0 {
        return Ok(0);
    }

    // Second pass: detect empty section headers to remove
    // Check if `catalog:` section is now empty
    let mut catalog_header_line = None;
    let mut catalog_has_remaining = false;
    let mut catalogs_header_line = None;
    let mut named_catalog_headers: Vec<(usize, String)> = Vec::new();
    let mut named_catalog_has_remaining: HashSet<String> = HashSet::new();

    section = YamlSection::Other;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_end_matches('\r');

        if !trimmed.is_empty() && !trimmed.starts_with(' ') && !trimmed.starts_with('#') {
            if trimmed == "catalog:" || trimmed.starts_with("catalog:") && trimmed[8..].trim().is_empty() {
                section = YamlSection::DefaultCatalog;
                catalog_header_line = Some(i);
                continue;
            } else if trimmed == "catalogs:" || trimmed.starts_with("catalogs:") && trimmed[9..].trim().is_empty() {
                section = YamlSection::CatalogsHeader;
                catalogs_header_line = Some(i);
                continue;
            } else {
                section = YamlSection::Other;
                continue;
            }
        }

        match &section {
            YamlSection::DefaultCatalog => {
                if extract_yaml_key(trimmed, 2).is_some() && !remove_lines.contains(&i) {
                    catalog_has_remaining = true;
                }
            }
            YamlSection::CatalogsHeader => {
                if let Some(key) = extract_yaml_key(trimmed, 2) {
                    named_catalog_headers.push((i, key.to_string()));
                    section = YamlSection::NamedCatalog(key.to_string());
                }
            }
            YamlSection::NamedCatalog(catalog_name) => {
                if let Some(key) = extract_yaml_key(trimmed, 4) {
                    if !remove_lines.contains(&i) {
                        named_catalog_has_remaining.insert(catalog_name.clone());
                    }
                    let _ = key;
                } else if let Some(key) = extract_yaml_key(trimmed, 2) {
                    named_catalog_headers.push((i, key.to_string()));
                    section = YamlSection::NamedCatalog(key.to_string());
                }
            }
            YamlSection::Other => {}
        }
    }

    // Remove empty catalog: header
    if !catalog_has_remaining {
        if let Some(line_idx) = catalog_header_line {
            remove_lines.insert(line_idx);
        }
    }

    // Remove empty named catalog headers
    for (line_idx, name) in &named_catalog_headers {
        if !named_catalog_has_remaining.contains(name) {
            remove_lines.insert(*line_idx);
        }
    }

    // Remove catalogs: header if all named catalogs are empty
    if !named_catalog_headers.is_empty()
        && named_catalog_has_remaining.is_empty()
    {
        if let Some(line_idx) = catalogs_header_line {
            remove_lines.insert(line_idx);
        }
    }

    // Build output, skipping removed lines
    let result: Vec<&str> = lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !remove_lines.contains(i))
        .map(|(_, line)| *line)
        .collect();

    let mut output = result.join("\n");

    // Normalize line endings if the original used \r\n
    if line_ending == "\r\n" {
        output = output.replace('\n', "\r\n");
    }

    std::fs::write(&yaml_path, &output)
        .with_context(|| format!("Failed to write {}", yaml_path.display()))?;

    Ok(removed_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_default_catalog() {
        let yaml = r#"
packages:
  - "packages/*"
catalog:
  react: "^18.2.0"
  lodash: "^4.17.21"
"#;
        let ws: PnpmWorkspaceYaml = serde_yaml::from_str(yaml).unwrap();
        let catalogs = WorkspaceCatalogs {
            default: ws.catalog,
            named: ws.catalogs,
        };
        assert!(catalogs.has_default_entry("react"));
        assert!(catalogs.has_default_entry("lodash"));
        assert!(!catalogs.has_default_entry("express"));
    }

    #[test]
    fn parse_named_catalogs() {
        let yaml = r#"
catalogs:
  react16:
    react: "^16.7.0"
    react-dom: "^16.7.0"
  react17:
    react: "^17.0.2"
"#;
        let ws: PnpmWorkspaceYaml = serde_yaml::from_str(yaml).unwrap();
        let catalogs = WorkspaceCatalogs {
            default: ws.catalog,
            named: ws.catalogs,
        };
        assert!(catalogs.has_catalog("react16"));
        assert!(catalogs.has_named_entry("react16", "react"));
        assert!(!catalogs.has_named_entry("react16", "express"));
        assert!(!catalogs.has_catalog("react18"));
    }

    #[test]
    fn find_dependency_across_catalogs() {
        let yaml = r#"
catalog:
  react: "^18.2.0"
catalogs:
  legacy:
    react: "^16.0.0"
"#;
        let ws: PnpmWorkspaceYaml = serde_yaml::from_str(yaml).unwrap();
        let catalogs = WorkspaceCatalogs {
            default: ws.catalog,
            named: ws.catalogs,
        };
        let found = catalogs.find_dependency("react");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0], None); // default
        assert_eq!(found[1], Some("legacy".to_string()));
    }

    #[test]
    fn all_entries() {
        let yaml = r#"
catalog:
  react: "^18.2.0"
catalogs:
  legacy:
    jquery: "^3.6.0"
"#;
        let ws: PnpmWorkspaceYaml = serde_yaml::from_str(yaml).unwrap();
        let catalogs = WorkspaceCatalogs {
            default: ws.catalog,
            named: ws.catalogs,
        };
        let entries = catalogs.all_entries();
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&CatalogEntry {
            catalog_name: None,
            dependency_name: "react".to_string(),
        }));
        assert!(entries.contains(&CatalogEntry {
            catalog_name: Some("legacy".to_string()),
            dependency_name: "jquery".to_string(),
        }));
    }

    #[test]
    fn empty_catalogs() {
        let yaml = r#"
packages:
  - "packages/*"
"#;
        let ws: PnpmWorkspaceYaml = serde_yaml::from_str(yaml).unwrap();
        let catalogs = WorkspaceCatalogs {
            default: ws.catalog,
            named: ws.catalogs,
        };
        assert!(!catalogs.has_default_entry("anything"));
        assert!(catalogs.all_entries().is_empty());
        assert!(catalogs.find_dependency("react").is_empty());
    }

    #[test]
    fn extract_yaml_key_unquoted() {
        assert_eq!(extract_yaml_key("  react: \"^18.2.0\"", 2), Some("react"));
        assert_eq!(extract_yaml_key("    lodash: \"^4.0.0\"", 4), Some("lodash"));
        assert_eq!(extract_yaml_key("  @types/react: \"^18.0.0\"", 2), Some("@types/react"));
    }

    #[test]
    fn extract_yaml_key_quoted() {
        assert_eq!(extract_yaml_key("  \"@scope/pkg\": \"^1.0.0\"", 2), Some("@scope/pkg"));
        assert_eq!(extract_yaml_key("  '@scope/pkg': \"^1.0.0\"", 2), Some("@scope/pkg"));
    }

    #[test]
    fn extract_yaml_key_wrong_indent() {
        assert_eq!(extract_yaml_key("    react: \"^18.2.0\"", 2), None);
        assert_eq!(extract_yaml_key("react: \"^18.2.0\"", 2), None);
    }

    #[test]
    fn extract_yaml_key_not_a_kv() {
        assert_eq!(extract_yaml_key("  # comment", 2), None);
        assert_eq!(extract_yaml_key("", 2), None);
        assert_eq!(extract_yaml_key("  ", 2), None);
    }

    fn write_temp_yaml(content: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let yaml_path = dir.path().join("pnpm-workspace.yaml");
        std::fs::write(&yaml_path, content).unwrap();
        (dir, yaml_path)
    }

    #[test]
    fn remove_default_catalog_entry() {
        let yaml = "packages:\n  - \"packages/*\"\n\ncatalog:\n  react: \"^18.2.0\"\n  lodash: \"^4.17.21\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: None,
            dependency_name: "lodash".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(result.contains("react: \"^18.2.0\""));
        assert!(!result.contains("lodash"));
        assert!(result.contains("catalog:"));
    }

    #[test]
    fn remove_all_default_entries_removes_header() {
        let yaml = "packages:\n  - \"packages/*\"\n\ncatalog:\n  lodash: \"^4.17.21\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: None,
            dependency_name: "lodash".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(!result.contains("catalog:"));
        assert!(result.contains("packages:"));
    }

    #[test]
    fn remove_named_catalog_entry() {
        let yaml = "catalogs:\n  legacy:\n    react: \"^16.0.0\"\n    jquery: \"^3.6.0\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: Some("legacy".to_string()),
            dependency_name: "jquery".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(result.contains("react: \"^16.0.0\""));
        assert!(!result.contains("jquery"));
        assert!(result.contains("catalogs:"));
        assert!(result.contains("legacy:"));
    }

    #[test]
    fn remove_all_named_entries_removes_headers() {
        let yaml = "catalogs:\n  legacy:\n    jquery: \"^3.6.0\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: Some("legacy".to_string()),
            dependency_name: "jquery".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(!result.contains("catalogs:"));
        assert!(!result.contains("legacy:"));
        assert!(!result.contains("jquery"));
    }

    #[test]
    fn remove_scoped_package() {
        let yaml = "catalog:\n  \"@types/react\": \"^18.0.0\"\n  react: \"^18.2.0\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: None,
            dependency_name: "@types/react".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(!result.contains("@types/react"));
        assert!(result.contains("react: \"^18.2.0\""));
    }

    #[test]
    fn remove_nothing_when_no_match() {
        let yaml = "catalog:\n  react: \"^18.2.0\"\n";
        let (dir, _yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: None,
            dependency_name: "lodash".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn preserves_comments_and_other_sections() {
        let yaml = "# workspace config\npackages:\n  - \"packages/*\"\n\ncatalog:\n  react: \"^18.2.0\"\n  # keep lodash\n  lodash: \"^4.17.21\"\n  leftpad: \"^1.0.0\"\n";
        let (dir, yaml_path) = write_temp_yaml(yaml);

        let entries = vec![CatalogEntry {
            catalog_name: None,
            dependency_name: "leftpad".to_string(),
        }];

        let count = remove_catalog_entries(dir.path(), &entries).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(&yaml_path).unwrap();
        assert!(result.contains("# workspace config"));
        assert!(result.contains("# keep lodash"));
        assert!(result.contains("react: \"^18.2.0\""));
        assert!(result.contains("lodash: \"^4.17.21\""));
        assert!(!result.contains("leftpad"));
    }
}
