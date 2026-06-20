//! `gdtest.toml` — persistent config file (internal).
//!
//! Searched upward from the project root (like Cargo.toml). CLI flags and
//! existing env vars always win; config values are injected as env vars
//! before clap parses, so the priority is:
//!
//!   CLI flag > environment variable > gdtest.toml > built-in default

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

// ── Config types ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GdtestConfig {
    pub godot_exe: Option<String>,
    pub jobs: Option<usize>,
    pub display_mode: Option<String>,
    /// "pretty" | "tap" | "json" | "junit"
    pub reporter: Option<String>,
    /// "file" | "test"
    pub isolation: Option<String>,
    pub timeout: Option<u64>,
    pub timeout_warn: Option<u64>,
    pub fail_fast: Option<bool>,
    pub retries: Option<u32>,
    pub harness: Option<String>,
    #[serde(default)]
    pub include_tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    /// Glob patterns (relative to project root) to exclude from coverage instrumentation.
    /// Example: `["addons/**", "test/**"]`
    #[serde(default)]
    pub coverage_exclude: Vec<String>,
    /// Plugin scripts to load in the worker (internal).
    /// Each entry is a `res://` path to a GDScript that extends
    /// `Gdt.GdtReporter` or `Gdt.GdtFixture`.
    /// Example: `["res://addons/my-plugin/reporter.gd"]`
    #[serde(default)]
    pub plugins: Vec<String>,

    /// Glob patterns (relative to project root) used to discover test files
    /// when walking directories. Defaults to `["test/**/*.gd", "**/*.test.gd"]`.
    #[serde(default)]
    pub patterns: Vec<String>,

    #[serde(default)]
    pub profile: HashMap<String, ProfileConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileConfig {
    pub jobs: Option<usize>,
    pub display_mode: Option<String>,
    pub reporter: Option<String>,
    pub isolation: Option<String>,
    pub timeout: Option<u64>,
    pub timeout_warn: Option<u64>,
    pub fail_fast: Option<bool>,
    pub retries: Option<u32>,
    pub harness: Option<String>,
    #[serde(default)]
    pub include_tags: Vec<String>,
    #[serde(default)]
    pub exclude_tags: Vec<String>,
    #[serde(default)]
    pub coverage_exclude: Vec<String>,
}

impl GdtestConfig {
    pub fn apply_profile(&mut self, name: &str) -> anyhow::Result<()> {
        let p = self
            .profile
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("gdtest.toml: profile `{name}` not found"))?;
        macro_rules! overlay {
            ($field:ident) => {
                if p.$field.is_some() {
                    self.$field = p.$field;
                }
            };
        }
        overlay!(jobs);
        overlay!(display_mode);
        overlay!(reporter);
        overlay!(isolation);
        overlay!(timeout);
        overlay!(timeout_warn);
        overlay!(fail_fast);
        overlay!(retries);
        overlay!(harness);
        if !p.include_tags.is_empty() {
            self.include_tags = p.include_tags;
        }
        if !p.exclude_tags.is_empty() {
            self.exclude_tags = p.exclude_tags;
        }
        if !p.coverage_exclude.is_empty() {
            self.coverage_exclude = p.coverage_exclude;
        }
        Ok(())
    }
}

// ── File loading ──────────────────────────────────────────────────────────────

fn find_config_file(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join("gdtest.toml");
        if candidate.exists() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn load(project_path: &Path) -> anyhow::Result<Option<GdtestConfig>> {
    let start = project_path
        .canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf());
    let Some(path) = find_config_file(&start) else {
        return Ok(None);
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("gdtest: failed to read {}: {e}", path.display()))?;
    let cfg: GdtestConfig = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("gdtest: {} parse error: {e}", path.display()))?;
    Ok(Some(cfg))
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn pre_parse_str_arg(flag: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let eq_prefix = format!("{flag}=");
    let mut i = 1;
    while i < args.len() {
        if args[i] == flag && i + 1 < args.len() {
            return Some(args[i + 1].clone());
        }
        if let Some(val) = args[i].strip_prefix(&eq_prefix) {
            return Some(val.to_string());
        }
        i += 1;
    }
    None
}

fn set_if_absent(key: &str, val: Option<String>) {
    if let Some(v) = val {
        if std::env::var(key).is_err() {
            std::env::set_var(key, v);
        }
    }
}

/// Load `gdtest.toml` and inject config values as env vars before clap parses.
/// CLI flags and pre-existing env vars always win (env vars are only set when absent).
pub fn apply_env_defaults() -> anyhow::Result<()> {
    let project_path = pre_parse_str_arg("--project-path").unwrap_or_else(|| ".".to_string());
    let profile = pre_parse_str_arg("--profile");

    let Some(mut cfg) = load(Path::new(&project_path))? else {
        return Ok(());
    };
    if let Some(name) = profile {
        cfg.apply_profile(&name)?;
    }

    set_if_absent("GODOT_EXE", cfg.godot_exe);
    set_if_absent("GDTEST_DISPLAY_MODE", cfg.display_mode);
    set_if_absent("GDTEST_REPORTER", cfg.reporter);
    set_if_absent("GDTEST_ISOLATION", cfg.isolation);
    set_if_absent("GDTEST_TIMEOUT", cfg.timeout.map(|t| t.to_string()));
    set_if_absent(
        "GDTEST_TIMEOUT_WARN",
        cfg.timeout_warn.map(|t| t.to_string()),
    );
    set_if_absent("GDTEST_FAIL_FAST", cfg.fail_fast.map(|f| f.to_string()));
    set_if_absent("GDTEST_RETRIES", cfg.retries.map(|r| r.to_string()));
    set_if_absent("GDTEST_HARNESS", cfg.harness);
    set_if_absent("GDTEST_JOBS", cfg.jobs.map(|j| j.to_string()));
    if !cfg.include_tags.is_empty() && std::env::var("GDTEST_INCLUDE_TAGS").is_err() {
        std::env::set_var("GDTEST_INCLUDE_TAGS", cfg.include_tags.join(","));
    }
    if !cfg.exclude_tags.is_empty() && std::env::var("GDTEST_EXCLUDE_TAGS").is_err() {
        std::env::set_var("GDTEST_EXCLUDE_TAGS", cfg.exclude_tags.join(","));
    }
    if !cfg.coverage_exclude.is_empty() && std::env::var("GDTEST_COVERAGE_EXCLUDE").is_err() {
        std::env::set_var("GDTEST_COVERAGE_EXCLUDE", cfg.coverage_exclude.join(","));
    }
    Ok(())
}

/// Return the `plugins` list from `gdtest.toml`, if any. Returns an empty Vec
/// if there is no config file or the file has no `plugins` entry.
pub fn load_plugins(project_path: &str) -> Vec<String> {
    let Ok(cfg) = load(Path::new(project_path)) else {
        return Vec::new();
    };
    cfg.map(|c| c.plugins).unwrap_or_default()
}

/// Return the `patterns` list from `gdtest.toml`, or the built-in defaults.
/// Never returns an empty Vec — callers can always pass the result to `resolve`.
pub fn load_patterns(project_path: &str) -> Vec<String> {
    let Ok(Some(cfg)) = load(Path::new(project_path)) else {
        return gdtest_analysis::DEFAULT_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();
    };
    if cfg.patterns.is_empty() {
        gdtest_analysis::DEFAULT_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        cfg.patterns
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{GdtestConfig, ProfileConfig};

    #[test]
    fn apply_profile_overlays_display_mode() {
        let mut cfg = GdtestConfig {
            display_mode: Some("headless".to_string()),
            profile: HashMap::from([(
                "gui".to_string(),
                ProfileConfig {
                    display_mode: Some("windowed".to_string()),
                    ..ProfileConfig::default()
                },
            )]),
            ..GdtestConfig::default()
        };

        cfg.apply_profile("gui").unwrap();

        assert_eq!(cfg.display_mode.as_deref(), Some("windowed"));
    }
}
