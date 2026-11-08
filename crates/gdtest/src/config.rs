//! gdtest's persistent settings: the `[test]` section of `gdsuite.toml`, or
//! `gdtest.toml`.
//!
//! Searched upward from the project root (like Cargo.toml), stopping at the
//! Godot project root. In each folder, a `gdsuite.toml` with a `[test]`
//! section comes first; a `gdtest.toml` next to it is then ignored, with a
//! warning, since one tool's settings belong in one file. CLI flags and
//! existing env vars always win. The effective priority is:
//!
//!   CLI flag > environment variable > config file > built-in default

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

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
    /// Plugin scripts to load in the worker.
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
            .ok_or_else(|| anyhow::anyhow!("gdtest config: profile `{name}` not found"))?;
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

/// The file gdtest's settings come from.
#[derive(Debug, PartialEq, Eq)]
enum ConfigSource {
    /// The `[test]` section of a `gdsuite.toml`.
    Suite(PathBuf),
    /// A `gdtest.toml`.
    Own(PathBuf),
}

const SUITE_FILE: &str = "gdsuite.toml";
const SUITE_SECTION: &str = "test";
const OWN_FILE: &str = "gdtest.toml";

/// Whether the warning about finding both files was printed: the config is
/// loaded several times per run.
static WARNED_BOTH: AtomicBool = AtomicBool::new(false);

/// Walk up from `start` looking for gdtest's settings, stopping at the Godot
/// project root so that an unrelated config in a parent directory is never
/// picked up.
fn find_config_file(start: &Path) -> anyhow::Result<Option<ConfigSource>> {
    let mut dir = start.to_path_buf();
    loop {
        let suite = dir.join(SUITE_FILE);
        let own = dir.join(OWN_FILE);
        if suite_section(&suite)?.is_some() {
            if own.exists() && !WARNED_BOTH.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "gdtest: warning: {} has a [{SUITE_SECTION}] section, so {} is ignored. \
                     Keep gdtest's settings in one of them.",
                    suite.display(),
                    own.display()
                );
            }
            return Ok(Some(ConfigSource::Suite(suite)));
        }
        if own.exists() {
            return Ok(Some(ConfigSource::Own(own)));
        }
        if dir.join("project.godot").exists() || !dir.pop() {
            return Ok(None);
        }
    }
}

/// The `[test]` section of a `gdsuite.toml`, if the file exists and has one.
fn suite_section(path: &Path) -> anyhow::Result<Option<toml::Value>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("gdtest: failed to read {}: {e}", path.display()))?;
    let mut table: toml::Table = toml::from_str(&text)
        .map_err(|e| anyhow::anyhow!("gdtest: {} parse error: {e}", path.display()))?;
    Ok(table.remove(SUITE_SECTION))
}

fn load(project_path: &Path) -> anyhow::Result<Option<GdtestConfig>> {
    let start = project_path
        .canonicalize()
        .unwrap_or_else(|_| project_path.to_path_buf());
    match find_config_file(&start)? {
        None => Ok(None),
        Some(ConfigSource::Suite(path)) => {
            let section = suite_section(&path)?.unwrap_or(toml::Value::Table(Default::default()));
            let cfg: GdtestConfig = section.try_into().map_err(|e| {
                anyhow::anyhow!(
                    "gdtest: {} [{SUITE_SECTION}] parse error: {e}",
                    path.display()
                )
            })?;
            Ok(Some(cfg))
        }
        Some(ConfigSource::Own(path)) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("gdtest: failed to read {}: {e}", path.display()))?;
            let cfg: GdtestConfig = toml::from_str(&text)
                .map_err(|e| anyhow::anyhow!("gdtest: {} parse error: {e}", path.display()))?;
            Ok(Some(cfg))
        }
    }
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

    use super::{ConfigSource, GdtestConfig, ProfileConfig, find_config_file, load};

    #[test]
    fn config_search_stops_at_the_project_root() {
        let root = std::env::temp_dir().join(format!("gdtest-cfg-{}", std::process::id()));
        let project = root.join("project");
        let subdir = project.join("sub");
        std::fs::create_dir_all(&subdir).unwrap();
        std::fs::write(root.join("gdtest.toml"), "").unwrap();
        std::fs::write(project.join("project.godot"), "").unwrap();

        assert_eq!(find_config_file(&subdir).unwrap(), None);

        std::fs::write(project.join("gdtest.toml"), "").unwrap();
        assert_eq!(
            find_config_file(&subdir).unwrap(),
            Some(ConfigSource::Own(project.join("gdtest.toml")))
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_gdsuite_toml_test_section_comes_before_gdtest_toml() {
        let project = std::env::temp_dir().join(format!("gdtest-suite-{}", std::process::id()));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("project.godot"), "").unwrap();
        std::fs::write(project.join("gdtest.toml"), "timeout = 1000\n").unwrap();

        // A gdsuite.toml without [test] leaves gdtest.toml in charge.
        std::fs::write(project.join("gdsuite.toml"), "[project]\nroot = \".\"\n").unwrap();
        assert_eq!(load(&project).unwrap().unwrap().timeout, Some(1000));

        std::fs::write(
            project.join("gdsuite.toml"),
            "[project]\nroot = \".\"\n\n[test]\nexclude = [\"test/manual/**\"]\n\n[test.profile.ci]\nfail_fast = true\n",
        )
        .unwrap();
        assert_eq!(
            find_config_file(&project).unwrap(),
            Some(ConfigSource::Suite(project.join("gdsuite.toml")))
        );
        let mut cfg = load(&project).unwrap().unwrap();
        // gdtest.toml is ignored, not merged.
        assert_eq!(cfg.timeout, None);
        assert_eq!(cfg.exclude, vec!["test/manual/**"]);
        cfg.apply_profile("ci").unwrap();
        assert_eq!(cfg.fail_fast, Some(true));

        std::fs::remove_dir_all(project).unwrap();
    }

    #[test]
    fn an_unknown_key_in_the_test_section_is_an_error() {
        let project = std::env::temp_dir().join(format!("gdtest-suite-bad-{}", std::process::id()));
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(project.join("project.godot"), "").unwrap();
        std::fs::write(project.join("gdsuite.toml"), "[test]\ntimout = 5\n").unwrap();

        let err = load(&project).unwrap_err().to_string();
        assert!(err.contains("gdsuite.toml [test] parse error"), "{err}");

        std::fs::remove_dir_all(project).unwrap();
    }

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
