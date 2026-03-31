use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::packages::{
    DependencyKind, Package, is_catalog_ref, is_special_protocol, parse_catalog_ref,
};
use crate::rules::catalog_entry_exists::{CatalogEntryExistsIssue, MissingCatalog};
use crate::rules::no_direct_version::NoDirectVersionIssue;
use crate::rules::no_uncataloged_dependency::NoUncatalogedDependencyIssue;
use crate::rules::unused_catalog_entry::UnusedCatalogEntryIssue;
use crate::rules::{Filter, IssuesList};
use crate::workspace::{CatalogEntry, PnpmWorkspaceYaml, WorkspaceCatalogs};

/// Describes a single version replacement for fixing no-direct-version.
#[derive(Debug, Clone)]
pub struct VersionReplacement {
    pub package_path: PathBuf,
    pub dependency_name: String,
    pub kind: DependencyKind,
    pub catalog_ref: String,
}

/// Describes a new entry to add to the default catalog for fixing no-uncataloged-dependency.
#[derive(Debug, Clone)]
pub struct CatalogAddition {
    pub dependency_name: String,
    pub version: String,
}

/// All fix-related data returned alongside issues.
pub struct FixActions {
    pub unused_entries: Vec<CatalogEntry>,
    pub version_replacements: Vec<VersionReplacement>,
    pub catalog_additions: Vec<CatalogAddition>,
    pub catalog_addition_replacements: Vec<VersionReplacement>,
}

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
    rule_filter: Filter,
    package_filter: &Filter,
    dependency_filter: &Filter,
) -> (IssuesList, FixActions) {
    let mut issues = IssuesList::new(rule_filter);
    let mut version_replacements = Vec::new();
    let mut catalog_additions_raw: Vec<(CatalogAddition, VersionReplacement)> = Vec::new();

    // Track used catalog entries for unused-catalog-entry rule
    let mut used_entries = catalogs.all_entries();

    for pkg in packages {
        let pkg_name = match &pkg.package_type {
            crate::packages::PackageType::Root => "(root)".to_string(),
            crate::packages::PackageType::Workspace(name) => name.clone(),
        };
        let is_ignored = package_filter.is_ignored(&pkg_name);

        for dep in pkg.all_dependencies() {
            if !is_ignored && dependency_filter.is_ignored(&dep.name) {
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
                            } else if !is_ignored {
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
                                if !is_ignored {
                                    issues.add(
                                        pkg.package_type.clone(),
                                        Box::new(CatalogEntryExistsIssue {
                                            dependency_name: dep.name.clone(),
                                            catalog_ref: dep.version.clone(),
                                            kind: dep.kind,
                                            missing: MissingCatalog::NamedCatalog(name.clone()),
                                        }),
                                    );
                                }
                            } else if catalogs.has_named_entry(name, &dep.name) {
                                // Mark as used
                                used_entries.retain(|e| {
                                    !(e.catalog_name.as_deref() == Some(name)
                                        && e.dependency_name == dep.name)
                                });
                            } else if !is_ignored {
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

                    if !is_ignored {
                        // Prefer default catalog, otherwise first named catalog
                        let catalog_ref = if found_in.contains(&None) {
                            "catalog:".to_string()
                        } else {
                            format!("catalog:{}", found_in[0].as_ref().unwrap())
                        };

                        version_replacements.push(VersionReplacement {
                            package_path: pkg.path.clone(),
                            dependency_name: dep.name.clone(),
                            kind: dep.kind,
                            catalog_ref,
                        });

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
                } else if !is_ignored {
                    issues.add(
                        pkg.package_type.clone(),
                        Box::new(NoUncatalogedDependencyIssue {
                            dependency_name: dep.name.clone(),
                            version: dep.version.clone(),
                            kind: dep.kind,
                        }),
                    );
                    catalog_additions_raw.push((
                        CatalogAddition {
                            dependency_name: dep.name.clone(),
                            version: dep.version.clone(),
                        },
                        VersionReplacement {
                            package_path: pkg.path.clone(),
                            dependency_name: dep.name.clone(),
                            kind: dep.kind,
                            catalog_ref: "catalog:".to_string(),
                        },
                    ));
                }
            }
        }
    }

    // Collect unused entries before emitting warnings
    let unused_entries: Vec<CatalogEntry> = if issues.is_rule_ignored("unused-catalog-entry") {
        Vec::new()
    } else {
        used_entries.iter().cloned().collect()
    };

    // Clear version replacements if rule is ignored
    let version_replacements = if issues.is_rule_ignored("no-direct-version") {
        Vec::new()
    } else {
        version_replacements
    };

    // Deduplicate catalog additions: same dep+version → 1 addition, N replacements.
    // If versions conflict for the same dep → skip entirely (not auto-fixable).
    let (catalog_additions, catalog_addition_replacements) = if issues
        .is_rule_ignored("no-uncataloged-dependency")
    {
        (Vec::new(), Vec::new())
    } else {
        let mut seen: HashMap<String, String> = HashMap::new();
        let mut conflicting: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (addition, _) in &catalog_additions_raw {
            match seen.entry(addition.dependency_name.clone()) {
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(addition.version.clone());
                }
                std::collections::hash_map::Entry::Occupied(e) => {
                    if e.get() != &addition.version {
                        conflicting.insert(addition.dependency_name.clone());
                    }
                }
            }
        }

        let mut additions = Vec::new();
        let mut replacements = Vec::new();
        let mut added_deps: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (addition, replacement) in catalog_additions_raw {
            if conflicting.contains(&addition.dependency_name) {
                continue;
            }
            replacements.push(replacement);
            if added_deps.insert(addition.dependency_name.clone()) {
                additions.push(addition);
            }
        }
        (additions, replacements)
    };

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

    (
        issues,
        FixActions {
            unused_entries,
            version_replacements,
            catalog_additions,
            catalog_addition_replacements,
        },
    )
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

    #[test]
    fn direct_version_marks_catalog_entry_as_used() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        // Should report no-direct-version error but NOT unused-catalog-entry
        assert_eq!(issues.errors_count(), 1);
        assert_eq!(issues.warnings_count(), 0);

        let (_, issue) = issues.iter().next().unwrap();
        assert_eq!(issue.name(), "no-direct-version");
    }

    #[test]
    fn direct_version_no_false_unused_when_rule_excluded() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Exclude(vec!["no-direct-version".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        // With no-direct-version excluded, there should be zero issues —
        // the catalog entry is still considered used
        assert!(issues.is_empty());
    }

    #[test]
    fn returns_unused_entries() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0"), ("lodash", "^4.17.21")]);
        let packages = vec![make_package("app", vec![("react", "catalog:")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(fix.unused_entries.len(), 1);
        assert_eq!(fix.unused_entries[0].dependency_name, "lodash");
        assert_eq!(fix.unused_entries[0].catalog_name, None);
    }

    #[test]
    fn excluded_package_deps_still_mark_catalog_entries_as_used() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0"), ("lodash", "^4.17.21")]);
        let packages = vec![
            make_package("app", vec![("react", "catalog:")]),
            make_package("excluded-pkg", vec![("lodash", "catalog:")]),
        ];

        let (issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::Exclude(vec!["excluded-pkg".to_string()]),
            &Filter::None,
        );

        // lodash should NOT be reported as unused — excluded-pkg references it
        assert!(fix.unused_entries.is_empty());
        // No issues from excluded-pkg should be reported
        assert_eq!(issues.errors_count(), 0);
        assert_eq!(issues.warnings_count(), 0);
    }

    #[test]
    fn unused_entries_empty_when_rule_excluded() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0"), ("lodash", "^4.17.21")]);
        let packages = vec![make_package("app", vec![("react", "catalog:")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Exclude(vec!["unused-catalog-entry".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        assert!(fix.unused_entries.is_empty());
    }

    #[test]
    fn only_runs_specified_rules() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0"), ("lodash", "^4.17.21")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Only(vec!["no-direct-version".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        // Only no-direct-version should be reported, unused-catalog-entry should be filtered
        assert_eq!(issues.errors_count(), 1);
        assert_eq!(issues.warnings_count(), 0);

        let (_, issue) = issues.iter().next().unwrap();
        assert_eq!(issue.name(), "no-direct-version");
    }

    #[test]
    fn only_excludes_unspecified_rules() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0"), ("lodash", "^4.17.21")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Only(vec!["unused-catalog-entry".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        // no-direct-version should be filtered out, only unused-catalog-entry should remain
        assert_eq!(issues.errors_count(), 0);
        assert_eq!(issues.warnings_count(), 1);
        assert_eq!(fix.unused_entries.len(), 1);
    }

    #[test]
    fn collects_version_replacements_for_direct_versions() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(fix.version_replacements.len(), 1);
        assert_eq!(fix.version_replacements[0].dependency_name, "react");
        assert_eq!(fix.version_replacements[0].catalog_ref, "catalog:");
    }

    #[test]
    fn version_replacements_prefer_default_catalog() {
        let mut named = IndexMap::new();
        let mut legacy = IndexMap::new();
        legacy.insert("react".to_string(), "^16.0.0".to_string());
        named.insert("legacy".to_string(), legacy);

        let catalogs = WorkspaceCatalogs {
            default: {
                let mut m = IndexMap::new();
                m.insert("react".to_string(), "^18.2.0".to_string());
                m
            },
            named,
        };
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(fix.version_replacements.len(), 1);
        assert_eq!(fix.version_replacements[0].catalog_ref, "catalog:");
    }

    #[test]
    fn version_replacements_use_named_catalog_when_no_default() {
        let mut named = IndexMap::new();
        let mut legacy = IndexMap::new();
        legacy.insert("react".to_string(), "^16.0.0".to_string());
        named.insert("legacy".to_string(), legacy);

        let catalogs = WorkspaceCatalogs {
            default: IndexMap::new(),
            named,
        };
        let packages = vec![make_package("app", vec![("react", "^16.0.0")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(fix.version_replacements.len(), 1);
        assert_eq!(fix.version_replacements[0].catalog_ref, "catalog:legacy");
    }

    #[test]
    fn version_replacements_empty_when_rule_excluded() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Exclude(vec!["no-direct-version".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        assert!(fix.version_replacements.is_empty());
    }

    #[test]
    fn uncataloged_dependency_detected() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package("app", vec![("lodash", "^4.17.21")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(issues.warnings_count(), 1);
        let (_, issue) = issues.iter().next().unwrap();
        assert_eq!(issue.name(), "no-uncataloged-dependency");
    }

    #[test]
    fn uncataloged_dependency_skipped_when_in_catalog() {
        let catalogs = make_catalogs(vec![("react", "^18.2.0")]);
        let packages = vec![make_package("app", vec![("react", "^18.2.0")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        // Should only have no-direct-version, not no-uncataloged-dependency
        assert!(issues.iter().all(|(_, i)| i.name() == "no-direct-version"));
    }

    #[test]
    fn uncataloged_dependency_skipped_for_special_protocols() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package(
            "app",
            vec![("my-lib", "workspace:*"), ("my-link", "link:../lib")],
        )];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert!(issues.is_empty());
    }

    #[test]
    fn uncataloged_dependency_respects_rule_exclusion() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package("app", vec![("lodash", "^4.17.21")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Exclude(vec!["no-uncataloged-dependency".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        assert!(issues.is_empty());
    }

    #[test]
    fn uncataloged_dependency_respects_dependency_filter() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package("app", vec![("lodash", "^4.17.21")])];

        let (issues, _fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::Exclude(vec!["lodash".to_string()]),
        );

        assert!(issues.is_empty());
    }

    #[test]
    fn uncataloged_fix_collects_catalog_additions() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package("app", vec![("lodash", "^4.17.21")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        assert_eq!(fix.catalog_additions.len(), 1);
        assert_eq!(fix.catalog_additions[0].dependency_name, "lodash");
        assert_eq!(fix.catalog_additions[0].version, "^4.17.21");
        assert_eq!(fix.catalog_addition_replacements.len(), 1);
        assert_eq!(fix.catalog_addition_replacements[0].catalog_ref, "catalog:");
    }

    #[test]
    fn uncataloged_fix_deduplicates_same_version() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![
            make_package("app-a", vec![("lodash", "^4.17.21")]),
            make_package("app-b", vec![("lodash", "^4.17.21")]),
        ];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        // 1 catalog addition, 2 version replacements
        assert_eq!(fix.catalog_additions.len(), 1);
        assert_eq!(fix.catalog_addition_replacements.len(), 2);
    }

    #[test]
    fn uncataloged_fix_skips_conflicting_versions() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![
            make_package("app-a", vec![("lodash", "^4.17.21")]),
            make_package("app-b", vec![("lodash", "^4.17.20")]),
        ];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::None,
            &Filter::None,
            &Filter::None,
        );

        // Conflicting versions — skip fix entirely for this dep
        assert!(fix.catalog_additions.is_empty());
        assert!(fix.catalog_addition_replacements.is_empty());
    }

    #[test]
    fn uncataloged_fix_empty_when_rule_excluded() {
        let catalogs = make_catalogs(vec![]);
        let packages = vec![make_package("app", vec![("lodash", "^4.17.21")])];

        let (_issues, fix) = collect_issues(
            &packages,
            &catalogs,
            Filter::Exclude(vec!["no-uncataloged-dependency".to_string()]),
            &Filter::None,
            &Filter::None,
        );

        assert!(fix.catalog_additions.is_empty());
        assert!(fix.catalog_addition_replacements.is_empty());
    }
}
