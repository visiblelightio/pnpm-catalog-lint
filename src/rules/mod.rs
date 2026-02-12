pub mod catalog_entry_exists;
pub mod no_direct_version;
pub mod unused_catalog_entry;

use std::fmt;

use colored::Colorize;

use crate::packages::PackageType;

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
    ignored_rules: Vec<String>,
}

impl IssuesList {
    pub fn new(ignored_rules: Vec<String>) -> Self {
        Self {
            issues: Vec::new(),
            ignored_rules,
        }
    }

    pub fn add(&mut self, package_type: PackageType, issue: Box<dyn Issue>) {
        if !self.ignored_rules.contains(&issue.name().to_string()) {
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

    pub fn is_empty(&self) -> bool {
        self.issues.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &(PackageType, Box<dyn Issue>)> {
        self.issues.iter()
    }
}
