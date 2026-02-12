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
}
