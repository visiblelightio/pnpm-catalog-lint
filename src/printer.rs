use std::collections::BTreeMap;
use std::time::Duration;

use colored::Colorize;

use crate::packages::PackageType;
use crate::rules::IssuesList;

pub fn print_issues(issues: &IssuesList) {
    // Group issues by package
    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for (pkg_type, issue) in issues.iter() {
        let key = match pkg_type {
            PackageType::Root => "pnpm-workspace.yaml".to_string(),
            PackageType::Workspace(name) => name.clone(),
        };

        let line = format!(
            "  {}[{}] {}",
            issue.level(),
            issue.name().dimmed(),
            issue.message(),
        );

        grouped.entry(key).or_default().push(line);
    }

    for (pkg, lines) in &grouped {
        println!("{}", pkg.bold());
        for line in lines {
            println!("{line}");
        }
        println!();
    }
}

pub fn print_success() {
    println!("{}", "No issues found.".green().bold());
}

pub fn print_error(message: &str) {
    eprintln!("{} {message}", "error:".red().bold());
}

pub fn print_footer(issues: &IssuesList, duration: Duration) {
    let errors = issues.errors_count();
    let warnings = issues.warnings_count();
    let total = errors + warnings;
    let ms = duration.as_millis();

    let mut parts = Vec::new();
    if errors > 0 {
        parts.push(format!(
            "{}",
            format!("{errors} error{}", if errors == 1 { "" } else { "s" }).red()
        ));
    }
    if warnings > 0 {
        parts.push(format!(
            "{}",
            format!("{warnings} warning{}", if warnings == 1 { "" } else { "s" }).yellow()
        ));
    }

    println!(
        "Found {} ({}) in {ms}ms",
        format!("{total} issue{}", if total == 1 { "" } else { "s" }).bold(),
        parts.join(", "),
    );
}
