use std::collections::BTreeMap;
use std::time::Duration;

use colored::Colorize;
use serde::Serialize;

use crate::packages::PackageType;
use crate::rules::{IssueLevel, IssuesList};

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

pub fn print_fixed(count: usize) {
    let word = if count == 1 { "entry" } else { "entries" };
    println!(
        "{}",
        format!("Fixed {count} unused catalog {word}.")
            .green()
            .bold(),
    );
}

pub fn print_fixed_versions(count: usize) {
    let word = if count == 1 {
        "dependency"
    } else {
        "dependencies"
    };
    println!(
        "{}",
        format!("Fixed {count} {word} to use catalog: protocol.")
            .green()
            .bold(),
    );
}

pub fn print_fixed_catalog_additions(added: usize, replaced: usize) {
    let entry_word = if added == 1 { "entry" } else { "entries" };
    let dep_word = if replaced == 1 {
        "dependency"
    } else {
        "dependencies"
    };
    println!(
        "{}",
        format!("Added {added} catalog {entry_word}, fixed {replaced} {dep_word} to use catalog: protocol.")
            .green()
            .bold(),
    );
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

#[derive(Serialize)]
struct JsonIssue {
    package: String,
    level: &'static str,
    rule: String,
    message: String,
}

#[derive(Serialize)]
struct JsonSummary {
    total: usize,
    errors: usize,
    warnings: usize,
    duration_ms: u128,
}

#[derive(Serialize)]
struct JsonOutput {
    issues: Vec<JsonIssue>,
    summary: JsonSummary,
}

pub fn print_json(issues: &IssuesList, duration: Duration) {
    let json_issues: Vec<JsonIssue> = issues
        .iter()
        .map(|(pkg_type, issue)| {
            let package = match pkg_type {
                PackageType::Root => "pnpm-workspace.yaml".to_string(),
                PackageType::Workspace(name) => name.clone(),
            };
            JsonIssue {
                package,
                level: match issue.level() {
                    IssueLevel::Error => "error",
                    IssueLevel::Warning => "warning",
                },
                rule: issue.name().to_string(),
                message: issue.message(),
            }
        })
        .collect();

    let errors = issues.errors_count();
    let warnings = issues.warnings_count();

    let output = JsonOutput {
        issues: json_issues,
        summary: JsonSummary {
            total: errors + warnings,
            errors,
            warnings,
            duration_ms: duration.as_millis(),
        },
    };

    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
