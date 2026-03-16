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

    let (mut issues, unused_entries) = collect::collect_issues(
        &packages,
        &catalogs,
        args.rule_filter(),
        &args.ignore_packages,
        &args.ignore_dependencies,
    );

    if args.fix && !unused_entries.is_empty() {
        match workspace::remove_catalog_entries(&root, &unused_entries) {
            Ok(count) => {
                printer::print_fixed(count);
                issues.remove_by_rule("unused-catalog-entry");
            }
            Err(e) => {
                printer::print_error(&format!("Failed to fix: {e:#}"));
            }
        }
    }

    let duration = start.elapsed();

    if issues.is_empty() {
        if !args.fix {
            printer::print_success();
        }
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
