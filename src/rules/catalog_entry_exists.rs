use crate::packages::DependencyKind;
use crate::rules::{Issue, IssueLevel};

#[derive(Debug)]
pub enum MissingCatalog {
    /// "catalog:" used but dependency not in default catalog
    DefaultEntry,
    /// "catalog:<name>" used but named catalog doesn't exist
    NamedCatalog(String),
    /// "catalog:<name>" used, catalog exists, but dependency not in it
    NamedEntry(String),
}

pub struct CatalogEntryExistsIssue {
    pub dependency_name: String,
    #[allow(dead_code)]
    pub catalog_ref: String,
    pub kind: DependencyKind,
    pub missing: MissingCatalog,
}

impl Issue for CatalogEntryExistsIssue {
    fn name(&self) -> &str {
        "catalog-entry-exists"
    }

    fn level(&self) -> IssueLevel {
        IssueLevel::Error
    }

    fn message(&self) -> String {
        match &self.missing {
            MissingCatalog::DefaultEntry => {
                format!(
                    "'{}' references \"catalog:\" in {} but is not defined in the default catalog",
                    self.dependency_name, self.kind,
                )
            }
            MissingCatalog::NamedCatalog(name) => {
                format!(
                    "'{}' references \"catalog:{name}\" in {} but catalog \"{name}\" does not exist",
                    self.dependency_name, self.kind,
                )
            }
            MissingCatalog::NamedEntry(name) => {
                format!(
                    "'{}' references \"catalog:{name}\" in {} but is not defined in catalog \"{name}\"",
                    self.dependency_name, self.kind,
                )
            }
        }
    }

    fn why(&self) -> &str {
        "A catalog: reference must point to an existing entry in pnpm-workspace.yaml. Missing entries will cause pnpm install to fail."
    }
}
