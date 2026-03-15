use clap::Parser;

use crate::rules::RuleFilter;

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
    #[arg(long, conflicts_with = "only")]
    pub exclude: Vec<String>,

    /// Run only specified rules (can be specified multiple times)
    #[arg(long, conflicts_with = "exclude")]
    pub only: Vec<String>,

    /// Packages to ignore (can be specified multiple times)
    #[arg(long = "ignore-package")]
    pub ignore_packages: Vec<String>,

    /// Dependencies to ignore (can be specified multiple times)
    #[arg(long = "ignore-dependency")]
    pub ignore_dependencies: Vec<String>,

    /// Automatically fix issues (currently supports unused-catalog-entry)
    #[arg(long)]
    pub fix: bool,

    /// Exit with non-zero code on warnings
    #[arg(long)]
    pub fail_on_warnings: bool,
}

impl Args {
    pub fn rule_filter(&self) -> RuleFilter {
        if !self.only.is_empty() {
            RuleFilter::Only(self.only.clone())
        } else if !self.exclude.is_empty() {
            RuleFilter::Exclude(self.exclude.clone())
        } else {
            RuleFilter::None
        }
    }
}
