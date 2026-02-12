use crate::packages::DependencyKind;
use crate::rules::{Issue, IssueLevel};

pub struct NoDirectVersionIssue {
    pub dependency_name: String,
    pub version: String,
    pub kind: DependencyKind,
    /// Catalogs where this dependency is available (None = default, Some = named)
    pub available_in: Vec<Option<String>>,
}

impl Issue for NoDirectVersionIssue {
    fn name(&self) -> &str {
        "no-direct-version"
    }

    fn level(&self) -> IssueLevel {
        IssueLevel::Error
    }

    fn message(&self) -> String {
        let catalogs_desc = self
            .available_in
            .iter()
            .map(|c| match c {
                None => "default".to_string(),
                Some(name) => format!("\"{name}\""),
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            "'{}' uses \"{}\" in {} but is available in catalog: {catalogs_desc}. Use \"catalog:\" instead.",
            self.dependency_name, self.version, self.kind,
        )
    }

    fn why(&self) -> &str {
        "Dependencies available in the catalog should use the catalog: protocol to ensure version consistency across the monorepo."
    }
}
