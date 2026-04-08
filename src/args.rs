use clap::{Parser, ValueEnum};

use crate::rules::Filter;

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Parser, Debug)]
#[command(
    name = "pnpm-catalog-lint",
    about = "Lint pnpm workspaces to enforce the catalog: protocol",
    version
)]
pub struct Args {
    /// Path to the workspace root
    #[arg(default_value = ".")]
    pub path: String,

    /// Rules to exclude (can be specified multiple times)
    #[arg(long = "exclude-rule", conflicts_with = "only_rules")]
    pub exclude_rules: Vec<String>,

    /// Run only specified rules (can be specified multiple times)
    #[arg(long = "only-rule", conflicts_with = "exclude_rules")]
    pub only_rules: Vec<String>,

    /// Packages to exclude (can be specified multiple times)
    #[arg(long = "exclude-package", conflicts_with = "only_packages")]
    pub exclude_packages: Vec<String>,

    /// Run only on specified packages (can be specified multiple times)
    #[arg(long = "only-package", conflicts_with = "exclude_packages")]
    pub only_packages: Vec<String>,

    /// Dependencies to exclude (can be specified multiple times)
    #[arg(long = "exclude-dependency", conflicts_with = "only_dependencies")]
    pub exclude_dependencies: Vec<String>,

    /// Run only on specified dependencies (can be specified multiple times)
    #[arg(long = "only-dependency", conflicts_with = "exclude_dependencies")]
    pub only_dependencies: Vec<String>,

    /// Automatically fix issues (supports no-direct-version and unused-catalog-entry)
    #[arg(long)]
    pub fix: bool,

    /// Exit with non-zero code on warnings
    #[arg(long)]
    pub fail_on_warnings: bool,

    /// Output format
    #[arg(long, value_enum, default_value_t)]
    pub format: OutputFormat,

    /// Disable colored output
    #[arg(long)]
    pub no_color: bool,

    /// Suppress all output (exit code only)
    #[arg(long, short)]
    pub quiet: bool,
}

impl Args {
    pub fn rule_filter(&self) -> Filter {
        if !self.only_rules.is_empty() {
            Filter::Only(self.only_rules.clone())
        } else if !self.exclude_rules.is_empty() {
            Filter::Exclude(self.exclude_rules.clone())
        } else {
            Filter::None
        }
    }

    pub fn package_filter(&self) -> Filter {
        if !self.only_packages.is_empty() {
            Filter::Only(self.only_packages.clone())
        } else if !self.exclude_packages.is_empty() {
            Filter::Exclude(self.exclude_packages.clone())
        } else {
            Filter::None
        }
    }

    pub fn dependency_filter(&self) -> Filter {
        if !self.only_dependencies.is_empty() {
            Filter::Only(self.only_dependencies.clone())
        } else if !self.exclude_dependencies.is_empty() {
            Filter::Exclude(self.exclude_dependencies.clone())
        } else {
            Filter::None
        }
    }
}
