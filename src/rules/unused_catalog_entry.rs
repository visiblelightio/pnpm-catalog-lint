use crate::rules::{Issue, IssueLevel};

pub struct UnusedCatalogEntryIssue {
    pub dependency_name: String,
    /// None = default catalog, Some(name) = named catalog
    pub catalog_name: Option<String>,
    pub version: String,
}

impl Issue for UnusedCatalogEntryIssue {
    fn name(&self) -> &str {
        "unused-catalog-entry"
    }

    fn level(&self) -> IssueLevel {
        IssueLevel::Warning
    }

    fn message(&self) -> String {
        match &self.catalog_name {
            None => {
                format!(
                    "'{}' (\"{}\") in the default catalog is never referenced",
                    self.dependency_name, self.version,
                )
            }
            Some(name) => {
                format!(
                    "'{}' (\"{}\") in catalog \"{name}\" is never referenced",
                    self.dependency_name, self.version,
                )
            }
        }
    }

    fn why(&self) -> &str {
        "Unused catalog entries add noise to pnpm-workspace.yaml and may indicate stale dependencies that should be removed."
    }
}
