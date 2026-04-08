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

    if args.no_color {
        colored::control::set_override(false);
    }

    let is_json = matches!(args.format, args::OutputFormat::Json);
    let is_quiet = args.quiet;

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

    let (mut issues, fix) = collect::collect_issues(
        &packages,
        &catalogs,
        args.rule_filter(),
        &args.package_filter(),
        &args.dependency_filter(),
    );

    if args.fix && !fix.catalog_additions.is_empty() {
        match workspace::add_catalog_entries(&root, &fix.catalog_additions) {
            Ok(added) => match packages::replace_versions(&fix.catalog_addition_replacements) {
                Ok(replaced) => {
                    if !is_quiet && !is_json {
                        printer::print_fixed_catalog_additions(added, replaced);
                    }
                    issues.remove_by_rule("no-uncataloged-dependency");
                }
                Err(e) => {
                    printer::print_error(&format!("Failed to fix: {e:#}"));
                }
            },
            Err(e) => {
                printer::print_error(&format!("Failed to fix: {e:#}"));
            }
        }
    }

    if args.fix && !fix.version_replacements.is_empty() {
        match packages::replace_versions(&fix.version_replacements) {
            Ok(count) => {
                if !is_quiet && !is_json {
                    printer::print_fixed_versions(count);
                }
                issues.remove_by_rule("no-direct-version");
            }
            Err(e) => {
                printer::print_error(&format!("Failed to fix: {e:#}"));
            }
        }
    }

    if args.fix && !fix.unused_entries.is_empty() {
        match workspace::remove_catalog_entries(&root, &fix.unused_entries) {
            Ok(count) => {
                if !is_quiet && !is_json {
                    printer::print_fixed(count);
                }
                issues.remove_by_rule("unused-catalog-entry");
            }
            Err(e) => {
                printer::print_error(&format!("Failed to fix: {e:#}"));
            }
        }
    }

    let duration = start.elapsed();

    if issues.is_empty() {
        if !is_quiet {
            if is_json {
                printer::print_json(&issues, duration);
            } else if !args.fix {
                printer::print_success();
            }
        }
        process::exit(0);
    }

    if !is_quiet {
        if is_json {
            printer::print_json(&issues, duration);
        } else {
            printer::print_issues(&issues);
            printer::print_footer(&issues, duration);
        }
    }

    let has_errors = issues.errors_count() > 0;
    let has_failing_warnings = args.fail_on_warnings && issues.warnings_count() > 0;
    if has_errors || has_failing_warnings {
        process::exit(1);
    }
}
