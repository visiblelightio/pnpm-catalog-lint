use std::process;
use std::time::Instant;

use clap::Parser;

mod args;
mod collect;
mod packages;
mod printer;
mod rules;
mod workspace;

fn main() {
    let args = args::Args::parse();
    let start = Instant::now();

    let root = match std::path::Path::new(&args.path).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            printer::print_error(&format!("Invalid path '{}': {e}", args.path));
            process::exit(1);
        }
    };

    let (workspace_yaml, catalogs) = match workspace::parse_workspace(&root) {
        Ok(result) => result,
        Err(e) => {
            printer::print_error(&format!("{e:#}"));
            process::exit(1);
        }
    };

    let packages = match collect::collect_packages(&root, &workspace_yaml) {
        Ok(pkgs) => pkgs,
        Err(e) => {
            printer::print_error(&format!("{e:#}"));
            process::exit(1);
        }
    };

    let issues = collect::collect_issues(
        &packages,
        &catalogs,
        &args.ignore_rules,
        &args.ignore_packages,
        &args.ignore_dependencies,
    );

    let duration = start.elapsed();

    if issues.is_empty() {
        printer::print_success();
        process::exit(0);
    }

    printer::print_issues(&issues);
    printer::print_footer(&issues, duration);

    let has_errors = issues.errors_count() > 0;
    let has_failing_warnings = args.fail_on_warnings && issues.warnings_count() > 0;
    if has_errors || has_failing_warnings {
        process::exit(1);
    }
}
