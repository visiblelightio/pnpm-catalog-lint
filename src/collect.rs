use std::path::Path;

use anyhow::{Context, Result};

use crate::packages::{Package, is_catalog_ref, is_special_protocol, parse_catalog_ref};
use crate::rules::IssuesList;
use crate::rules::catalog_entry_exists::{CatalogEntryExistsIssue, MissingCatalog};
use crate::rules::no_direct_version::NoDirectVersionIssue;
use crate::rules::unused_catalog_entry::UnusedCatalogEntryIssue;
use crate::workspace::{PnpmWorkspaceYaml, WorkspaceCatalogs};

pub fn collect_packages(root: &Path, workspace: &PnpmWorkspaceYaml) -> Result<Vec<Package>> {
    let mut packages = Vec::new();

    // Load root package.json
    let root_pkg_path = root.join("package.json");
    if root_pkg_path.exists() {
        packages.push(
            Package::load(root, true)
                .with_context(|| format!("Failed to load root package at {}", root.display()))?,
        );
    }

    // Expand workspace package patterns
    for pattern in &workspace.packages {
        // Skip negated patterns
        if pattern.starts_with('!') {
            continue;
        }

        let full_pattern = root.join(pattern).to_string_lossy().to_string();
        let matches = glob::glob(&full_pattern)
            .with_context(|| format!("Invalid glob pattern: {pattern}"))?;

        for entry in matches {
            let entry = entry.with_context(|| format!("Glob error for pattern: {pattern}"))?;

            // entry could be a directory or a file matching the glob
            let dir = if entry.is_dir() {
                entry
            } else {
                continue;
            };

            // Skip if no package.json
            if !dir.join("package.json").exists() {
                continue;
            }

            // Skip root (already added)
            if dir == root {
                continue;
            }

            packages.push(
                Package::load(&dir, false)
                    .with_context(|| format!("Failed to load package at {}", dir.display()))?,
            );
        }
    }

    Ok(packages)
}

pub fn collect_issues(
    packages: &[Package],
    catalogs: &WorkspaceCatalogs,
    ignored_rules: &[String],
    ignored_packages: &[String],
    ignored_dependencies: &[String],
) -> IssuesList {
    let mut issues = IssuesList::new(ignored_rules.to_vec());

    // Track used catalog entries for unused-catalog-entry rule
    let mut used_entries = catalogs.all_entries();

    for pkg in packages {
        // Check if this package should be ignored
        let pkg_name = match &pkg.package_type {
            crate::packages::PackageType::Root => "(root)".to_string(),
            crate::packages::PackageType::Workspace(name) => name.clone(),
        };
        if ignored_packages.iter().any(|p| p == &pkg_name) {
            continue;
        }

        for dep in pkg.all_dependencies() {
            // Check if this dependency should be ignored
            if ignored_dependencies.iter().any(|d| d == &dep.name) {
                continue;
            }

            if is_catalog_ref(&dep.version) {
                // Dependency uses catalog: protocol — check if the entry exists
                let parsed = parse_catalog_ref(&dep.version);
                if let Some(catalog_name) = parsed {
                    match &catalog_name {
                        None => {
                            // Default catalog reference
                            if catalogs.has_default_entry(&dep.name) {
                                // Mark as used
                                used_entries.retain(|e| {
                                    !(e.catalog_name.is_none() && e.dependency_name == dep.name)
                                });
                            } else {
                                issues.add(
                                    pkg.package_type.clone(),
                                    Box::new(CatalogEntryExistsIssue {
                                        dependency_name: dep.name.clone(),
                                        catalog_ref: dep.version.clone(),
                                        kind: dep.kind,
                                        missing: MissingCatalog::DefaultEntry,
                                    }),
                                );
                            }
                        }
                        Some(name) => {
                            // Named catalog reference
                            if !catalogs.has_catalog(name) {
                                issues.add(
                                    pkg.package_type.clone(),
                                    Box::new(CatalogEntryExistsIssue {
                                        dependency_name: dep.name.clone(),
                                        catalog_ref: dep.version.clone(),
                                        kind: dep.kind,
                                        missing: MissingCatalog::NamedCatalog(name.clone()),
                                    }),
                                );
                            } else if catalogs.has_named_entry(name, &dep.name) {
                                // Mark as used
                                used_entries.retain(|e| {
                                    !(e.catalog_name.as_deref() == Some(name)
                                        && e.dependency_name == dep.name)
                                });
                            } else {
                                issues.add(
                                    pkg.package_type.clone(),
                                    Box::new(CatalogEntryExistsIssue {
                                        dependency_name: dep.name.clone(),
                                        catalog_ref: dep.version.clone(),
                                        kind: dep.kind,
                                        missing: MissingCatalog::NamedEntry(name.clone()),
                                    }),
                                );
                            }
                        }
                    }
                }
            } else if !is_special_protocol(&dep.version) {
                // Dependency uses a direct version — check if it's in any catalog
                let found_in = catalogs.find_dependency(&dep.name);
                if !found_in.is_empty() {
                    // Mark matching catalog entries as used (the dependency exists,
                    // it's just not using the catalog: protocol)
                    for catalog_name in &found_in {
                        used_entries.retain(|e| {
                            !(e.catalog_name == *catalog_name && e.dependency_name == dep.name)
                        });
                    }

                    issues.add(
                        pkg.package_type.clone(),
                        Box::new(NoDirectVersionIssue {
                            dependency_name: dep.name.clone(),
                            version: dep.version.clone(),
                            kind: dep.kind,
                            available_in: found_in,
                        }),
                    );
                }
            }
        }
    }

    // Emit unused catalog entry warnings
    for entry in &used_entries {
        if let Some(version) = catalogs.get_version(entry) {
            issues.add(
                crate::packages::PackageType::Root,
                Box::new(UnusedCatalogEntryIssue {
                    dependency_name: entry.dependency_name.clone(),
                    catalog_name: entry.catalog_name.clone(),
                    version: version.to_string(),
                }),
            );
        }
    }

    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::{Package, PackageJson, PackageType};
    use indexmap::IndexMap;
    use std::path::PathBuf;

    fn make_package(name: &str, deps: Vec<(&str, &str)>) -> Package {
        let mut dependencies = IndexMap::new();
        for (k, v) in deps {
            dependencies.insert(k.to_string(), v.to_string());
        }
        Package {
            path: PathBuf::from(format!("/fake/{name}")),
            package_type: PackageType::Workspace(name.to_string()),
            inner: PackageJson {
                name: Some(name.to_string()),
                dependencies,
                dev_dependencies: IndexMap::new(),
                peer_dependencies: IndexMap::new(),
                optional_dependencies: IndexMap::new(),
            },
        }
    }

    fn make_catalogs(default: Vec<(&str, &str)>) -> WorkspaceCatalogs {
        let mut map = IndexMap::new();
        for (k, v) in default {
            map.insert(k.to_string(), v.to_string());
        }
        WorkspaceCatalogs {
            default: map,
            named: IndexMap::new(),
        }
    }

    fn no_ignored() -> Vec<String> {
        vec![]
    }

    #[test]
    fn direct_version_marks_catalog_entry_as_used() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let issues = collect_issues(&packages, &catalogs, &no_ignored(), &no_ignored(), &no_ignored());

        // Should report no-direct-version error but NOT unused-catalog-entry
        assert_eq!(issues.errors_count(), 1);
        assert_eq!(issues.warnings_count(), 0);

        let (_, issue) = issues.iter().next().unwrap();
        assert_eq!(issue.name(), "no-direct-version");
    }

    #[test]
    fn direct_version_no_false_unused_when_rule_ignored() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let issues = collect_issues(
            &packages,
            &catalogs,
            &["no-direct-version".to_string()],
            &no_ignored(),
            &no_ignored(),
        );

        // With no-direct-version ignored, there should be zero issues —
        // the catalog entry is still considered used
        assert!(issues.is_empty());
    }
}
