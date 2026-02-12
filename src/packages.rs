use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PackageType {
    Root,
    Workspace(String),
}

impl std::fmt::Display for PackageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackageType::Root => write!(f, "(root)"),
            PackageType::Workspace(name) => write!(f, "{name}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Dependencies,
    DevDependencies,
    PeerDependencies,
    OptionalDependencies,
}

impl std::fmt::Display for DependencyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DependencyKind::Dependencies => write!(f, "dependencies"),
            DependencyKind::DevDependencies => write!(f, "devDependencies"),
            DependencyKind::PeerDependencies => write!(f, "peerDependencies"),
            DependencyKind::OptionalDependencies => write!(f, "optionalDependencies"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Dependency {
    pub name: String,
    pub version: String,
    pub kind: DependencyKind,
}

#[derive(Debug, Deserialize)]
pub struct PackageJson {
    pub name: Option<String>,
    #[serde(default)]
    pub dependencies: IndexMap<String, String>,
    #[serde(rename = "devDependencies", default)]
    pub dev_dependencies: IndexMap<String, String>,
    #[serde(rename = "peerDependencies", default)]
    pub peer_dependencies: IndexMap<String, String>,
    #[serde(rename = "optionalDependencies", default)]
    pub optional_dependencies: IndexMap<String, String>,
}

#[derive(Debug)]
pub struct Package {
    #[allow(dead_code)]
    pub path: PathBuf,
    pub package_type: PackageType,
    pub inner: PackageJson,
}

impl Package {
    pub fn load(dir: &Path, is_root: bool) -> Result<Self> {
        let pkg_path = dir.join("package.json");
        let content = std::fs::read_to_string(&pkg_path)
            .with_context(|| format!("Failed to read {}", pkg_path.display()))?;
        let inner: PackageJson = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", pkg_path.display()))?;

        let package_type = if is_root {
            PackageType::Root
        } else {
            PackageType::Workspace(inner.name.clone().unwrap_or_else(|| {
                dir.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string()
            }))
        };

        Ok(Self {
            path: dir.to_path_buf(),
            package_type,
            inner,
        })
    }

    pub fn all_dependencies(&self) -> Vec<Dependency> {
        let mut deps = Vec::new();
        for (name, version) in &self.inner.dependencies {
            deps.push(Dependency {
                name: name.clone(),
                version: version.clone(),
                kind: DependencyKind::Dependencies,
            });
        }
        for (name, version) in &self.inner.dev_dependencies {
            deps.push(Dependency {
                name: name.clone(),
                version: version.clone(),
                kind: DependencyKind::DevDependencies,
            });
        }
        for (name, version) in &self.inner.peer_dependencies {
            deps.push(Dependency {
                name: name.clone(),
                version: version.clone(),
                kind: DependencyKind::PeerDependencies,
            });
        }
        for (name, version) in &self.inner.optional_dependencies {
            deps.push(Dependency {
                name: name.clone(),
                version: version.clone(),
                kind: DependencyKind::OptionalDependencies,
            });
        }
        deps
    }
}

/// Returns true if the version string uses the catalog: protocol.
pub fn is_catalog_ref(version: &str) -> bool {
    version == "catalog:" || version.starts_with("catalog:")
}

/// Returns true if the version string uses a special protocol that should be skipped.
pub fn is_special_protocol(version: &str) -> bool {
    version.starts_with("workspace:")
        || version.starts_with("link:")
        || version.starts_with("file:")
        || version.starts_with("git:")
        || version.starts_with("git+")
        || version.starts_with("http:")
        || version.starts_with("https:")
}

/// Parse a catalog: reference to extract the catalog name.
/// - "catalog:" → Some(None) — default catalog
/// - "catalog:default" → Some(None) — default catalog (explicit)
/// - "catalog:react16" → Some(Some("react16"))
/// - "^1.0.0" → None — not a catalog ref
pub fn parse_catalog_ref(version: &str) -> Option<Option<String>> {
    if version == "catalog:" {
        return Some(None);
    }
    let name = version.strip_prefix("catalog:")?;
    if name.is_empty() || name == "default" {
        Some(None)
    } else {
        Some(Some(name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_catalog_ref() {
        assert!(is_catalog_ref("catalog:"));
        assert!(is_catalog_ref("catalog:default"));
        assert!(is_catalog_ref("catalog:react16"));
        assert!(!is_catalog_ref("^1.0.0"));
        assert!(!is_catalog_ref("workspace:*"));
        assert!(!is_catalog_ref("catalogued"));
    }

    #[test]
    fn test_is_special_protocol() {
        assert!(is_special_protocol("workspace:*"));
        assert!(is_special_protocol("workspace:^"));
        assert!(is_special_protocol("link:../utils"));
        assert!(is_special_protocol("file:../utils"));
        assert!(is_special_protocol("git+https://github.com/foo/bar.git"));
        assert!(is_special_protocol("https://example.com/foo.tgz"));
        assert!(!is_special_protocol("^1.0.0"));
        assert!(!is_special_protocol("catalog:"));
    }

    #[test]
    fn test_parse_catalog_ref() {
        assert_eq!(parse_catalog_ref("catalog:"), Some(None));
        assert_eq!(parse_catalog_ref("catalog:default"), Some(None));
        assert_eq!(
            parse_catalog_ref("catalog:react16"),
            Some(Some("react16".to_string()))
        );
        assert_eq!(parse_catalog_ref("^1.0.0"), None);
        assert_eq!(parse_catalog_ref("workspace:*"), None);
    }
}
