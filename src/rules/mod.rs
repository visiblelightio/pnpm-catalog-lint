pub mod catalog_entry_exists;
pub mod no_direct_version;
pub mod no_uncataloged_dependency;
pub mod unused_catalog_entry;

use std::fmt;

use colored::Colorize;

use crate::packages::PackageType;

pub enum Filter {
    None,
    Exclude(Vec<String>),
    Only(Vec<String>),
}

impl Filter {
    pub fn is_ignored(&self, rule_name: &str) -> bool {
        match self {
            Filter::None => false,
            Filter::Exclude(rules) => rules.iter().any(|r| r == rule_name),
            Filter::Only(rules) => !rules.iter().any(|r| r == rule_name),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueLevel {
    Error,
    Warning,
}

impl fmt::Display for IssueLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueLevel::Error => write!(f, "{}", "error".red().bold()),
            IssueLevel::Warning => write!(f, "{}", "warning".yellow().bold()),
        }
    }
}

pub trait Issue {
    fn name(&self) -> &str;
    fn level(&self) -> IssueLevel;
    fn message(&self) -> String;
    #[allow(dead_code)]
    fn why(&self) -> &str;
}

pub struct IssuesList {
    issues: Vec<(PackageType, Box<dyn Issue>)>,
    rule_filter: Filter,
}

impl IssuesList {
    pub fn new(rule_filter: Filter) -> Self {
        Self {
            issues: Vec::new(),
            rule_filter,
        }
    }

    pub fn is_rule_ignored(&self, rule_name: &str) -> bool {
        self.rule_filter.is_ignored(rule_name)
    }

    pub fn add(&mut self, package_type: PackageType, issue: Box<dyn Issue>) {
        if !self.rule_filter.is_ignored(issue.name()) {
            self.issues.push((package_type, issue));
        }
    }

    pub fn errors_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|(_, i)| i.level() == IssueLevel::Error)
            .count()
    }

    pub fn warnings_count(&self) -> usize {
        self.issues
            .iter()
            .filter(|(_, i)| i.level() == IssueLevel::Warning)
            .count()
    }

    pub fn remove_by_rule(&mut self, rule_name: &str) {
        self.issues.retain(|(_, issue)| issue.name() != rule_name);
    }

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(PackageType, Box<dyn Issue>)> {
        self.issues.iter()
    }
}
