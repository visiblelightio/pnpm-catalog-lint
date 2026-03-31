use crate::packages::DependencyKind;
use crate::rules::{Issue, IssueLevel};

pub struct NoUncatalogedDependencyIssue {
    pub dependency_name: String,
    pub version: String,
    pub kind: DependencyKind,
}

impl Issue for NoUncatalogedDependencyIssue {
    fn name(&self) -> &str {
        "no-uncataloged-dependency"
    }

    fn level(&self) -> IssueLevel {
        IssueLevel::Warning
    }

    fn message(&self) -> String {
        format!(
            "'{}' uses \"{}\" in {} but is not in any catalog.",
            self.dependency_name, self.version, self.kind,
        )
    }

    fn why(&self) -> &str {
        "All dependencies should be managed through the pnpm catalog for version consistency across the monorepo."
    }
}
