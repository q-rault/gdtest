//! `gdtest.toml` — persistent config file (internal).
//!
//! Searched upward from the project root (like Cargo.toml). CLI flags and
//! existing env vars always win. The effective priority is:
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
    /// Plugin scripts to load in the worker (internal).
    /// Each entry is a `res://` path to a GDScript that extends
    /// `Gdt.GdtReporter` or `Gdt.GdtFixture`.
    /// Example: `["res://addons/my-plugin/reporter.gd"]`
    #[serde(default)]
    pub plugins: Vec<String>,

    /// Glob patterns (relative to project root) used to discover test files
    /// when walking directories. Defaults to `["test/**/*.gd", "**/*.test.gd"]`.
    #[serde(default)]
    pub include: Vec<String>,
    /// Glob patterns (relative to project root) excluded from test discovery
    /// when walking directories. Explicit file targets still run.
    /// Example: `["test/helpers/**", "test/mocks/**"]`
    #[serde(default)]
    pub exclude: Vec<String>,

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
    pub exclude: Vec<String>,
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
        if !p.exclude.is_empty() {
            self.exclude = p.exclude;
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

/// Load `gdtest.toml` and apply the requested profile, if any.
pub fn load_resolved(
    project_path: &str,
    profile: Option<&str>,
) -> anyhow::Result<Option<GdtestConfig>> {
    let Some(mut cfg) = load(Path::new(project_path))? else {
        return Ok(None);
    };
    if let Some(name) = profile {
        cfg.apply_profile(name)?;
    }
    Ok(Some(cfg))
}

/// Return the `plugins` list from `gdtest.toml`, if any. Returns an empty Vec
/// if there is no config file or the file has no `plugins` entry.
pub fn load_plugins(project_path: &str) -> Vec<String> {
    let Ok(cfg) = load(Path::new(project_path)) else {
        return Vec::new();
    };
    cfg.map(|c| c.plugins).unwrap_or_default()
}

/// Return the `include` list from `gdtest.toml`, or the built-in defaults.
/// Never returns an empty Vec — callers can always pass the result to `resolve`.
pub fn load_include(project_path: &str) -> Vec<String> {
    let Ok(Some(cfg)) = load(Path::new(project_path)) else {
        return gdtest_analysis::DEFAULT_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect();
    };
    if cfg.include.is_empty() {
        gdtest_analysis::DEFAULT_PATTERNS
            .iter()
            .map(|s| s.to_string())
            .collect()
    } else {
        cfg.include
    }
}

/// Return the `exclude` discovery list from `gdtest.toml`, or an empty Vec.
pub fn load_exclude(project_path: &str) -> Vec<String> {
    let Ok(Some(cfg)) = load(Path::new(project_path)) else {
        return Vec::new();
    };
    cfg.exclude
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

    #[test]
    fn apply_profile_overlays_exclude() {
        let mut cfg = GdtestConfig {
            exclude: vec!["test/helpers/**".to_string()],
            profile: HashMap::from([(
                "ci".to_string(),
                ProfileConfig {
                    exclude: vec!["test/mocks/**".to_string()],
                    ..ProfileConfig::default()
                },
            )]),
            ..GdtestConfig::default()
        };

        cfg.apply_profile("ci").unwrap();

        assert_eq!(cfg.exclude, vec!["test/mocks/**"]);
    }

    #[test]
    fn patterns_key_is_rejected() {
        let text = r#"
patterns = ["test/**/*.gd"]
"#;

        let err = toml::from_str::<GdtestConfig>(text)
            .unwrap_err()
            .to_string();
        assert!(err.contains("unknown field `patterns`"));
    }
}
