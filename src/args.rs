use clap::Parser;

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

    /// Rules to ignore (can be specified multiple times)
    #[arg(long = "ignore-rule")]
    pub ignore_rules: Vec<String>,

    /// Packages to ignore (can be specified multiple times)
    #[arg(long = "ignore-package")]
    pub ignore_packages: Vec<String>,

    /// Dependencies to ignore (can be specified multiple times)
    #[arg(long = "ignore-dependency")]
    pub ignore_dependencies: Vec<String>,

    /// Exit with non-zero code on warnings
    #[arg(long)]
    pub fail_on_warnings: bool,
}
