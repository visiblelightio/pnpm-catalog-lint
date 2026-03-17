use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::Deserialize;
use serde::Serialize;

use crate::collect::VersionReplacement;

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

/// Detect the indentation string used in a JSON file.
/// Returns the whitespace prefix of the first indented line. Defaults to 2 spaces.
fn detect_indent(content: &str) -> String {
    for line in content.lines() {
        let stripped = line.trim_start();
        if stripped.is_empty() {
            continue;
        }
        let indent = &line[..line.len() - stripped.len()];
        if !indent.is_empty() {
            return indent.to_string();
        }
    }
    "  ".to_string()
}

/// Replace direct versions with catalog: references in package.json files.
/// Returns the number of replacements made.
pub fn replace_versions(replacements: &[VersionReplacement]) -> Result<usize> {
    let mut by_path: HashMap<&Path, Vec<&VersionReplacement>> = HashMap::new();
    for r in replacements {
        by_path.entry(&r.package_path).or_default().push(r);
    }

    let mut total = 0;

    for (dir, reps) in &by_path {
        let pkg_path = dir.join("package.json");
        let content = std::fs::read_to_string(&pkg_path)
            .with_context(|| format!("Failed to read {}", pkg_path.display()))?;

        let indent = detect_indent(&content);
        let has_trailing_newline = content.ends_with('\n');

        let mut value: serde_json::Value = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse {}", pkg_path.display()))?;

        let mut count = 0;
        for rep in reps {
            let section_key = rep.kind.to_string();
            if let Some(obj) = value.get_mut(&section_key).and_then(|v| v.as_object_mut())
                && obj.contains_key(&rep.dependency_name)
            {
                obj.insert(
                    rep.dependency_name.clone(),
                    serde_json::Value::String(rep.catalog_ref.clone()),
                );
                count += 1;
            }
        }

        if count > 0 {
            let formatter = serde_json::ser::PrettyFormatter::with_indent(indent.as_bytes());
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
            value
                .serialize(&mut ser)
                .with_context(|| format!("Failed to serialize {}", pkg_path.display()))?;

            let mut output = String::from_utf8(buf)
                .with_context(|| format!("Invalid UTF-8 in serialized {}", pkg_path.display()))?;

            if has_trailing_newline && !output.ends_with('\n') {
                output.push('\n');
            }

            std::fs::write(&pkg_path, &output)
                .with_context(|| format!("Failed to write {}", pkg_path.display()))?;

            total += count;
        }
    }

    Ok(total)
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

    #[test]
    fn test_detect_indent_two_spaces() {
        let content = "{\n  \"name\": \"test\"\n}\n";
        assert_eq!(detect_indent(content), "  ");
    }

    #[test]
    fn test_detect_indent_four_spaces() {
        let content = "{\n    \"name\": \"test\"\n}\n";
        assert_eq!(detect_indent(content), "    ");
    }

    #[test]
    fn test_detect_indent_tabs() {
        let content = "{\n\t\"name\": \"test\"\n}\n";
        assert_eq!(detect_indent(content), "\t");
    }

    #[test]
    fn test_detect_indent_default() {
        let content = "{\"name\":\"test\"}";
        assert_eq!(detect_indent(content), "  ");
    }

    #[test]
    fn replace_versions_default_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = r#"{
  "name": "test-app",
  "dependencies": {
    "react": "^18.2.0",
    "lodash": "^4.17.21"
  }
}
"#;
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:".to_string(),
        }];

        let count = replace_versions(&replacements).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(result.contains("\"react\": \"catalog:\""));
        assert!(result.contains("\"lodash\": \"^4.17.21\""));
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn replace_versions_named_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = "{\n  \"name\": \"test-app\",\n  \"dependencies\": {\n    \"react\": \"^16.0.0\"\n  }\n}\n";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:legacy".to_string(),
        }];

        let count = replace_versions(&replacements).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(result.contains("\"react\": \"catalog:legacy\""));
    }

    #[test]
    fn replace_versions_multiple_deps() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = "{\n  \"name\": \"test-app\",\n  \"dependencies\": {\n    \"react\": \"^18.2.0\"\n  },\n  \"devDependencies\": {\n    \"typescript\": \"^5.0.0\"\n  }\n}\n";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![
            VersionReplacement {
                package_path: dir.path().to_path_buf(),
                dependency_name: "react".to_string(),
                kind: DependencyKind::Dependencies,
                catalog_ref: "catalog:".to_string(),
            },
            VersionReplacement {
                package_path: dir.path().to_path_buf(),
                dependency_name: "typescript".to_string(),
                kind: DependencyKind::DevDependencies,
                catalog_ref: "catalog:".to_string(),
            },
        ];

        let count = replace_versions(&replacements).unwrap();
        assert_eq!(count, 2);

        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(result.contains("\"react\": \"catalog:\""));
        assert!(result.contains("\"typescript\": \"catalog:\""));
    }

    #[test]
    fn replace_versions_preserves_four_space_indent() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = "{\n    \"name\": \"test-app\",\n    \"dependencies\": {\n        \"react\": \"^18.2.0\"\n    }\n}\n";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:".to_string(),
        }];

        let count = replace_versions(&replacements).unwrap();
        assert_eq!(count, 1);

        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(result.contains("    \"name\""));
        assert!(result.contains("        \"react\": \"catalog:\""));
    }

    #[test]
    fn replace_versions_preserves_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        // With trailing newline
        let pkg_json = "{\n  \"dependencies\": {\n    \"react\": \"^18.2.0\"\n  }\n}\n";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:".to_string(),
        }];

        replace_versions(&replacements).unwrap();
        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn replace_versions_no_trailing_newline() {
        let dir = tempfile::tempdir().unwrap();
        // Without trailing newline
        let pkg_json = "{\n  \"dependencies\": {\n    \"react\": \"^18.2.0\"\n  }\n}";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:".to_string(),
        }];

        replace_versions(&replacements).unwrap();
        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn replace_versions_preserves_key_order() {
        let dir = tempfile::tempdir().unwrap();
        let pkg_json = "{\n  \"name\": \"test\",\n  \"version\": \"1.0.0\",\n  \"scripts\": {},\n  \"dependencies\": {\n    \"react\": \"^18.2.0\"\n  }\n}\n";
        std::fs::write(dir.path().join("package.json"), pkg_json).unwrap();

        let replacements = vec![VersionReplacement {
            package_path: dir.path().to_path_buf(),
            dependency_name: "react".to_string(),
            kind: DependencyKind::Dependencies,
            catalog_ref: "catalog:".to_string(),
        }];

        replace_versions(&replacements).unwrap();
        let result = std::fs::read_to_string(dir.path().join("package.json")).unwrap();

        // Verify key order is preserved (name before version before scripts before dependencies)
        let name_pos = result.find("\"name\"").unwrap();
        let version_pos = result.find("\"version\"").unwrap();
        let scripts_pos = result.find("\"scripts\"").unwrap();
        let deps_pos = result.find("\"dependencies\"").unwrap();
        assert!(name_pos < version_pos);
        assert!(version_pos < scripts_pos);
        assert!(scripts_pos < deps_pos);
    }
}
