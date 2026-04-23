use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationFramework {
    Gut,
    Gdunit4,
}

impl MigrationFramework {
    fn as_str(self) -> &'static str {
        match self {
            MigrationFramework::Gut => "gut",
            MigrationFramework::Gdunit4 => "gdunit4",
        }
    }

    fn title(self) -> &'static str {
        match self {
            MigrationFramework::Gut => "GUT Migration Report",
            MigrationFramework::Gdunit4 => "gdUnit4 Migration Report",
        }
    }

    fn report_json_name(self) -> &'static str {
        match self {
            MigrationFramework::Gut => "gut_migration_report.json",
            MigrationFramework::Gdunit4 => "gdunit4_migration_report.json",
        }
    }

    fn report_md_name(self) -> &'static str {
        match self {
            MigrationFramework::Gut => "gut_migration_report.md",
            MigrationFramework::Gdunit4 => "gdunit4_migration_report.md",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MigrationStatus {
    CandidateForExecution,
    RewriteIncomplete,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationAction {
    pub line: usize,
    pub pattern: String,
    pub replacement: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationIssue {
    pub line: usize,
    pub pattern: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationReport {
    pub mode: &'static str,
    pub framework: MigrationFramework,
    pub project_path: String,
    pub output_dir: String,
    pub summary: MigrationSummary,
    pub files: Vec<MigrationReportFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationSummary {
    pub discovered_files: usize,
    pub candidate_files: usize,
    pub incomplete_files: usize,
    pub rewritten_patterns: usize,
    pub blocked_patterns: usize,
    pub manual_followups: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MigrationReportFile {
    pub input: String,
    pub output: String,
    pub status: MigrationStatus,
    pub rewritten: Vec<MigrationAction>,
    pub blocked: Vec<MigrationIssue>,
    pub manual_followups: Vec<MigrationIssue>,
}

#[derive(Debug, Clone)]
struct MigrationOutput {
    source: Vec<u8>,
    status: MigrationStatus,
    rewritten: Vec<MigrationAction>,
    blocked: Vec<MigrationIssue>,
    manual_followups: Vec<MigrationIssue>,
}

pub fn run_gut(inputs: &[String], project_path: &Path, out_dir: &Path) -> Result<()> {
    run_framework(inputs, project_path, out_dir, MigrationFramework::Gut)
}

pub fn run_gdunit4(inputs: &[String], project_path: &Path, out_dir: &Path) -> Result<()> {
    run_framework(inputs, project_path, out_dir, MigrationFramework::Gdunit4)
}

fn run_framework(
    inputs: &[String],
    project_path: &Path,
    out_dir: &Path,
    framework: MigrationFramework,
) -> Result<()> {
    let project_abs = project_path
        .canonicalize()
        .with_context(|| format!("project path does not exist: {}", project_path.display()))?;
    let files = discover_framework_files(inputs, &project_abs, framework)?;
    if files.is_empty() {
        return Err(anyhow!(
            "no {} test files found in the provided inputs",
            framework.as_str()
        ));
    }

    fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut report_files = Vec::new();
    for file in &files {
        let res_path = gdtest_discovery::to_res_path(file, &project_abs)?;
        let rel = file
            .strip_prefix(&project_abs)
            .with_context(|| format!("{} is outside {}", file.display(), project_abs.display()))?;
        let dest = out_dir.join(rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
        }
        let result = migrate_file(file, &res_path, framework)?;
        fs::write(&dest, &result.source).with_context(|| format!("writing {}", dest.display()))?;

        report_files.push(MigrationReportFile {
            input: rel.to_string_lossy().replace('\\', "/"),
            output: dest
                .strip_prefix(out_dir)
                .unwrap_or(&dest)
                .to_string_lossy()
                .replace('\\', "/"),
            status: result.status,
            rewritten: result.rewritten,
            blocked: result.blocked,
            manual_followups: result.manual_followups,
        });
    }

    let summary = MigrationSummary {
        discovered_files: report_files.len(),
        candidate_files: report_files
            .iter()
            .filter(|f| matches!(f.status, MigrationStatus::CandidateForExecution))
            .count(),
        incomplete_files: report_files
            .iter()
            .filter(|f| matches!(f.status, MigrationStatus::RewriteIncomplete))
            .count(),
        rewritten_patterns: report_files.iter().map(|f| f.rewritten.len()).sum(),
        blocked_patterns: report_files.iter().map(|f| f.blocked.len()).sum(),
        manual_followups: report_files.iter().map(|f| f.manual_followups.len()).sum(),
    };

    let report = MigrationReport {
        mode: "parallel_files",
        framework,
        project_path: project_abs.display().to_string(),
        output_dir: out_dir.display().to_string(),
        summary,
        files: report_files,
    };

    write_report_files(out_dir, &report)?;

    println!(
        "gdtest migrate {}: wrote {} file(s) to {} ({} candidate, {} incomplete)",
        framework.as_str(),
        report.summary.discovered_files,
        out_dir.display(),
        report.summary.candidate_files,
        report.summary.incomplete_files,
    );
    println!(
        "gdtest migrate {}: report {} and {}",
        framework.as_str(),
        out_dir.join(framework.report_md_name()).display(),
        out_dir.join(framework.report_json_name()).display(),
    );
    Ok(())
}

fn migrate_file(
    path: &Path,
    res_path: &str,
    framework: MigrationFramework,
) -> Result<MigrationOutput> {
    match framework {
        MigrationFramework::Gut => {
            let result = gdtest_discovery::migrate_gut_file(path, res_path)?;
            Ok(MigrationOutput {
                source: result.source,
                status: match result.status {
                    gdtest_discovery::GutMigrationStatus::CandidateForExecution => {
                        MigrationStatus::CandidateForExecution
                    }
                    gdtest_discovery::GutMigrationStatus::RewriteIncomplete => {
                        MigrationStatus::RewriteIncomplete
                    }
                },
                rewritten: result
                    .rewritten
                    .into_iter()
                    .map(|item| MigrationAction {
                        line: item.line,
                        pattern: item.pattern,
                        replacement: item.replacement,
                    })
                    .collect(),
                blocked: result
                    .blocked
                    .into_iter()
                    .map(|item| MigrationIssue {
                        line: item.line,
                        pattern: item.pattern,
                        message: item.message,
                    })
                    .collect(),
                manual_followups: result
                    .manual_followups
                    .into_iter()
                    .map(|item| MigrationIssue {
                        line: item.line,
                        pattern: item.pattern,
                        message: item.message,
                    })
                    .collect(),
            })
        }
        MigrationFramework::Gdunit4 => {
            let result = gdtest_discovery::migrate_gdunit4_file(path, res_path)?;
            Ok(MigrationOutput {
                source: result.source,
                status: match result.status {
                    gdtest_discovery::GdUnit4MigrationStatus::CandidateForExecution => {
                        MigrationStatus::CandidateForExecution
                    }
                    gdtest_discovery::GdUnit4MigrationStatus::RewriteIncomplete => {
                        MigrationStatus::RewriteIncomplete
                    }
                },
                rewritten: result
                    .rewritten
                    .into_iter()
                    .map(|item| MigrationAction {
                        line: item.line,
                        pattern: item.pattern,
                        replacement: item.replacement,
                    })
                    .collect(),
                blocked: result
                    .blocked
                    .into_iter()
                    .map(|item| MigrationIssue {
                        line: item.line,
                        pattern: item.pattern,
                        message: item.message,
                    })
                    .collect(),
                manual_followups: result
                    .manual_followups
                    .into_iter()
                    .map(|item| MigrationIssue {
                        line: item.line,
                        pattern: item.pattern,
                        message: item.message,
                    })
                    .collect(),
            })
        }
    }
}

fn write_report_files(out_dir: &Path, report: &MigrationReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    fs::write(out_dir.join(report.framework.report_json_name()), json).with_context(|| {
        format!(
            "writing {}",
            out_dir.join(report.framework.report_json_name()).display()
        )
    })?;
    fs::write(
        out_dir.join(report.framework.report_md_name()),
        render_markdown_report(report),
    )
    .with_context(|| {
        format!(
            "writing {}",
            out_dir.join(report.framework.report_md_name()).display()
        )
    })?;
    Ok(())
}

fn render_markdown_report(report: &MigrationReport) -> String {
    let mut md = String::new();
    md.push_str(&format!("# {}\n\n", report.framework.title()));
    md.push_str(&format!(
        "- Mode: `{}`\n- Framework: `{}`\n- Project: `{}`\n- Output: `{}`\n\n",
        report.mode,
        report.framework.as_str(),
        report.project_path,
        report.output_dir
    ));
    md.push_str("## Summary\n\n");
    md.push_str(&format!(
        "- Discovered files: {}\n- Candidate for execution: {}\n- Rewrite incomplete: {}\n- Rewritten patterns: {}\n- Blocked patterns: {}\n- Manual follow-ups: {}\n\n",
        report.summary.discovered_files,
        report.summary.candidate_files,
        report.summary.incomplete_files,
        report.summary.rewritten_patterns,
        report.summary.blocked_patterns,
        report.summary.manual_followups,
    ));
    md.push_str("## Files\n\n");
    for file in &report.files {
        md.push_str(&format!(
            "### `{}` -> `{}`\n\n- Status: `{}`\n",
            file.input,
            file.output,
            match file.status {
                MigrationStatus::CandidateForExecution => "candidate_for_execution",
                MigrationStatus::RewriteIncomplete => "rewrite_incomplete",
            }
        ));
        if file.rewritten.is_empty() {
            md.push_str("- Rewritten patterns: none\n");
        } else {
            md.push_str("- Rewritten patterns:\n");
            for item in &file.rewritten {
                md.push_str(&format!(
                    "  - line {}: `{}` -> `{}`\n",
                    item.line, item.pattern, item.replacement
                ));
            }
        }
        if file.blocked.is_empty() {
            md.push_str("- Blocked patterns: none\n");
        } else {
            md.push_str("- Blocked patterns:\n");
            for item in &file.blocked {
                md.push_str(&format!(
                    "  - line {}: `{}`: {}\n",
                    item.line, item.pattern, item.message
                ));
            }
        }
        if file.manual_followups.is_empty() {
            md.push_str("- Manual follow-ups: none\n\n");
        } else {
            md.push_str("- Manual follow-ups:\n");
            for item in &file.manual_followups {
                md.push_str(&format!(
                    "  - line {}: `{}`: {}\n",
                    item.line, item.pattern, item.message
                ));
            }
            md.push('\n');
        }
    }
    md
}

fn discover_framework_files(
    inputs: &[String],
    project_abs: &Path,
    framework: MigrationFramework,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for input in inputs {
        if let Some(res_path) = input.strip_prefix("res://") {
            let abs = project_abs.join(res_path);
            if abs.is_file() && is_framework_test_file(&abs, framework)? {
                files.push(abs);
            }
            continue;
        }
        let raw = PathBuf::from(input);
        let abs = if raw.is_absolute() {
            raw
        } else {
            project_abs.join(raw)
        };
        let abs = abs.canonicalize().unwrap_or(abs);
        if abs.is_file() {
            if abs.extension().and_then(|e| e.to_str()) == Some("gd")
                && is_framework_test_file(&abs, framework)?
            {
                files.push(abs);
            }
            continue;
        }
        if abs.is_dir() {
            for entry in WalkDir::new(&abs).follow_links(false) {
                let entry = entry.with_context(|| format!("walking {}", abs.display()))?;
                if !entry.file_type().is_file() {
                    continue;
                }
                if entry.path().extension().and_then(|e| e.to_str()) != Some("gd") {
                    continue;
                }
                if is_framework_test_file(entry.path(), framework)? {
                    files.push(entry.path().to_path_buf());
                }
            }
            continue;
        }
        return Err(anyhow!("path not found: {}", abs.display()));
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn is_framework_test_file(path: &Path, framework: MigrationFramework) -> Result<bool> {
    match framework {
        MigrationFramework::Gut => gdtest_discovery::is_gut_test_file(path),
        MigrationFramework::Gdunit4 => gdtest_discovery::is_gdunit4_test_file(path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("gdtest-{label}-{nonce}-{}", std::process::id()));
        dir
    }

    #[test]
    fn writes_parallel_tree_and_reports() {
        let project = temp_dir("gut-migrate-project");
        let out = project.join(".gdtest_rewrite").join("gut");
        let nested = project.join("test").join("combat");
        fs::create_dir_all(&nested).unwrap();
        let source_path = nested.join("test_attack.gd");
        fs::write(
            &source_path,
            br#"extends GutTest

func test_attack():
    var dbl = double(AttackSystem).new()
    assert_signal_emitted(self, "done")
"#,
        )
        .unwrap();

        run_gut(&["test".to_string()], &project, &out).unwrap();

        let generated = out.join("test").join("combat").join("test_attack.gd");
        assert!(generated.exists());
        let generated_text = fs::read_to_string(&generated).unwrap();
        assert!(generated_text.contains("extends GdTest"));
        assert!(generated_text.contains("mock(AttackSystem)"));
        assert!(generated_text.contains("assert_signal(self, \"done\")"));

        let original_text = fs::read_to_string(&source_path).unwrap();
        assert!(original_text.contains("extends GutTest"));

        let json_report = fs::read_to_string(out.join("gut_migration_report.json")).unwrap();
        assert!(json_report.contains("\"framework\": \"gut\""));
        assert!(json_report.contains("\"status\": \"candidate_for_execution\""));
        let md_report = fs::read_to_string(out.join("gut_migration_report.md")).unwrap();
        assert!(md_report.contains("test/combat/test_attack.gd"));

        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn incomplete_files_are_reported_and_overwritten_in_place() {
        let project = temp_dir("gut-migrate-incomplete");
        let out = project.join(".gdtest_rewrite").join("gut");
        let nested = project.join("test");
        fs::create_dir_all(&nested).unwrap();
        let source_path = nested.join("test_signal_watch.gd");
        fs::write(
            &source_path,
            br#"extends GutTest

func test_watch():
    watch_signals(self)
    assert_signal_emitted(self, "done")
"#,
        )
        .unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("stale.txt"), "keep").unwrap();

        run_gut(&["test".to_string()], &project, &out).unwrap();
        let generated = fs::read_to_string(out.join("test").join("test_signal_watch.gd")).unwrap();
        assert!(generated.starts_with("# gdtest migrate gut: manual follow-up required"));
        assert_eq!(fs::read_to_string(out.join("stale.txt")).unwrap(), "keep");

        let json_report = fs::read_to_string(out.join("gut_migration_report.json")).unwrap();
        assert!(json_report.contains("\"status\": \"rewrite_incomplete\""));

        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn writes_gdunit4_parallel_tree_and_reports() {
        let project = temp_dir("gdunit4-migrate-project");
        let out = project.join(".gdtest_rewrite").join("gdunit4");
        let nested = project.join("test").join("combat");
        fs::create_dir_all(&nested).unwrap();
        let source_path = nested.join("DamageSuite.gd");
        fs::write(
            &source_path,
            br#"class_name DamageSuite
extends GdUnitTestSuite

func test_attack() -> void:
    verify(mock_ai, 2).fire()
    var runner := scene_runner("res://ui/panel.tscn")
"#,
        )
        .unwrap();

        run_gdunit4(&["test".to_string()], &project, &out).unwrap();

        let generated = out.join("test").join("combat").join("DamageSuite.gd");
        assert!(generated.exists());
        let generated_text = fs::read_to_string(&generated).unwrap();
        assert!(!generated_text.contains("class_name DamageSuite"));
        assert!(generated_text.contains("extends GdTestCompatGdUnit"));
        assert!(generated_text.contains("assert_call_count(mock_ai, \"fire\", 2)"));
        assert!(generated_text.contains("scene_runner(load(\"res://ui/panel.tscn\"))"));

        let original_text = fs::read_to_string(&source_path).unwrap();
        assert!(original_text.contains("class_name DamageSuite"));
        assert!(original_text.contains("extends GdUnitTestSuite"));

        let json_report = fs::read_to_string(out.join("gdunit4_migration_report.json")).unwrap();
        assert!(json_report.contains("\"framework\": \"gdunit4\""));
        assert!(json_report.contains("\"status\": \"candidate_for_execution\""));
        let md_report = fs::read_to_string(out.join("gdunit4_migration_report.md")).unwrap();
        assert!(md_report.contains("test/combat/DamageSuite.gd"));

        fs::remove_dir_all(project).ok();
    }

    #[test]
    fn gdunit4_incomplete_files_are_reported() {
        let project = temp_dir("gdunit4-migrate-incomplete");
        let out = project.join(".gdtest_rewrite").join("gdunit4");
        let nested = project.join("test");
        fs::create_dir_all(&nested).unwrap();
        let source_path = nested.join("BlockedSuite.gd");
        fs::write(
            &source_path,
            br#"extends GdUnitTestSuite

@GdUnitTestParameters([tuple("a", "b")])
func test_watch() -> void:
    monitor_signals(self)
"#,
        )
        .unwrap();
        fs::create_dir_all(&out).unwrap();
        fs::write(out.join("stale.txt"), "keep").unwrap();

        run_gdunit4(&["test".to_string()], &project, &out).unwrap();
        let generated = fs::read_to_string(out.join("test").join("BlockedSuite.gd")).unwrap();
        assert!(generated.starts_with("# gdtest migrate gdunit4: manual follow-up required"));
        assert_eq!(fs::read_to_string(out.join("stale.txt")).unwrap(), "keep");

        let json_report = fs::read_to_string(out.join("gdunit4_migration_report.json")).unwrap();
        assert!(json_report.contains("\"status\": \"rewrite_incomplete\""));

        fs::remove_dir_all(project).ok();
    }
}
