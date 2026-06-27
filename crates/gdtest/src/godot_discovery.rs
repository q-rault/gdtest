use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Version {
    major: u32,
    minor: u32,
    patch: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VersionHint {
    major: u32,
    minor: Option<u32>,
}

#[derive(Debug, Clone)]
struct Candidate {
    command: OsString,
    display: String,
}

#[derive(Debug, Clone)]
struct ProbedCandidate {
    candidate: Candidate,
    version: Option<Version>,
}

#[derive(Debug)]
pub struct DiscoveryResult {
    pub path: String,
    pub warning: Option<String>,
}

pub fn resolve(project_path: &Path) -> Result<DiscoveryResult> {
    let hint = project_version_hint(project_path);
    let mut candidates = collect_candidates()?;
    dedupe_candidates(&mut candidates);
    let probed = probe_candidates(&candidates);
    choose_candidate(probed, hint)
}

fn choose_candidate(
    probed: Vec<ProbedCandidate>,
    hint: Option<VersionHint>,
) -> Result<DiscoveryResult> {
    let mut best_compatible: Option<(i64, ProbedCandidate)> = None;
    let mut best_unprobed: Option<Candidate> = None;
    let mut incompatible_versions = Vec::new();

    for pc in probed {
        match pc.version {
            Some(version) => {
                if version.major != 4 {
                    incompatible_versions.push((pc.candidate.display.clone(), version));
                    continue;
                }
                let score = compatibility_score(version, hint);
                match &best_compatible {
                    Some((best_score, _)) if score >= *best_score => {}
                    _ => best_compatible = Some((score, pc)),
                }
            }
            None => {
                if best_unprobed.is_none() {
                    best_unprobed = Some(pc.candidate);
                }
            }
        }
    }

    if let Some((_, best)) = best_compatible {
        let warning = hint.and_then(|hint| {
            best.version.and_then(|version| {
                hint.minor.and_then(|minor| {
                    if version.minor == minor {
                        None
                    } else {
                        Some(format!(
                            "gdtest: auto-selected Godot {} (project hints {}.{})",
                            best.candidate.display, hint.major, minor
                        ))
                    }
                })
            })
        });
        return Ok(DiscoveryResult {
            path: best.candidate.display,
            warning,
        });
    }

    if let Some(candidate) = best_unprobed {
        return Ok(DiscoveryResult {
            path: candidate.display.clone(),
            warning: Some(format!(
                "gdtest: using unverified Godot candidate {}; set --godot-exe to pin a specific binary",
                candidate.display
            )),
        });
    }

    let msg = discovery_failure_message(incompatible_versions);
    Err(anyhow!(msg))
}

fn discovery_failure_message(incompatible_versions: Vec<(String, Version)>) -> String {
    let searched_commands = common_command_names().join(", ");
    let searched_locations = common_location_roots()
        .into_iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let mut msg = format!(
        "gdtest: could not find a compatible Godot 4 executable.\nSearched command names: {searched_commands}\nSearched locations: {searched_locations}\nFix with --godot-exe, GODOT_EXE, or gdtest.toml `godot_exe = \"...\"`."
    );
    if !incompatible_versions.is_empty() {
        let found = incompatible_versions
            .into_iter()
            .map(|(path, version)| {
                format!(
                    "{path} ({}.{}.{})",
                    version.major, version.minor, version.patch
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        msg.push_str(&format!("\nFound incompatible candidates: {found}"));
    }
    msg
}

fn project_version_hint(project_path: &Path) -> Option<VersionHint> {
    let text = fs::read_to_string(project_path.join("project.godot")).ok()?;
    let re = Regex::new(r#""(\d+)\.(\d+)""#).ok()?;
    let caps = re.captures(&text)?;
    let major = caps.get(1)?.as_str().parse().ok()?;
    let minor = caps.get(2)?.as_str().parse().ok()?;
    Some(VersionHint {
        major,
        minor: Some(minor),
    })
}

fn collect_candidates() -> Result<Vec<Candidate>> {
    let mut candidates = Vec::new();
    for name in common_command_names() {
        candidates.push(Candidate {
            command: OsString::from(name),
            display: name.to_string(),
        });
    }
    for path in common_location_candidates()? {
        candidates.push(Candidate {
            display: path.display().to_string(),
            command: path.into_os_string(),
        });
    }
    Ok(candidates)
}

fn common_command_names() -> Vec<&'static str> {
    #[cfg(not(windows))]
    let names = vec!["godot", "godot4"];
    #[cfg(windows)]
    let mut names = vec!["godot", "godot4"];
    #[cfg(windows)]
    {
        names.extend(["godot4.exe", "godot4.bat"]);
    }
    names
}

fn common_location_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        roots.push(PathBuf::from(r"C:\Program Files\Godot"));
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Godot"));
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
            roots.push(local.join("Programs").join("Godot"));
        }
    }
    #[cfg(target_os = "macos")]
    {
        roots.push(PathBuf::from(
            "/Applications/Godot.app/Contents/MacOS/Godot",
        ));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(
                home.join("Applications")
                    .join("Godot.app")
                    .join("Contents")
                    .join("MacOS")
                    .join("Godot"),
            );
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        roots.push(PathBuf::from("/usr/bin/godot"));
        roots.push(PathBuf::from("/usr/bin/godot4"));
        roots.push(PathBuf::from("/usr/local/bin/godot"));
        roots.push(PathBuf::from("/usr/local/bin/godot4"));
        if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
            roots.push(home.join("Applications"));
        }
    }
    roots
}

fn common_location_candidates() -> Result<Vec<PathBuf>> {
    let mut candidates = Vec::new();
    #[cfg(windows)]
    {
        for root in common_location_roots() {
            collect_windows_candidates(&root, &mut candidates)?;
        }
    }
    #[cfg(target_os = "macos")]
    {
        for root in common_location_roots() {
            if root.exists() {
                candidates.push(root);
            }
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        for root in common_location_roots() {
            if root.is_file() {
                candidates.push(root);
            } else if root.is_dir() {
                collect_appimages(&root, &mut candidates)?;
            }
        }
    }
    Ok(candidates)
}

#[cfg(windows)]
fn collect_windows_candidates(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if root.is_file() {
        out.push(root.to_path_buf());
        return Ok(());
    }
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            maybe_push_windows_binary(&path, out);
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        for child in fs::read_dir(&path)? {
            let child = child?;
            maybe_push_windows_binary(&child.path(), out);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn maybe_push_windows_binary(path: &Path, out: &mut Vec<PathBuf>) {
    let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
        return;
    };
    let lower = name.to_ascii_lowercase();
    if lower.contains("godot") && lower.ends_with(".exe") {
        out.push(path.to_path_buf());
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn collect_appimages(root: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        let lower = name.to_ascii_lowercase();
        if lower.contains("godot") && lower.ends_with(".appimage") {
            out.push(path);
        }
    }
    Ok(())
}

fn dedupe_candidates(candidates: &mut Vec<Candidate>) {
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| seen.insert(candidate.display.clone()));
}

fn probe_candidates(candidates: &[Candidate]) -> Vec<ProbedCandidate> {
    candidates
        .iter()
        .cloned()
        .map(|candidate| {
            let version = probe_version(&candidate.command);
            ProbedCandidate { candidate, version }
        })
        .collect()
}

fn probe_version(command: &OsString) -> Option<Version> {
    let output = Command::new(command).arg("--version").output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    parse_version_output(&format!("{stdout}\n{stderr}"))
}

fn parse_version_output(text: &str) -> Option<Version> {
    let re = Regex::new(r"(\d+)\.(\d+)(?:\.(\d+))?").ok()?;
    let caps = re.captures(text)?;
    Some(Version {
        major: caps.get(1)?.as_str().parse().ok()?,
        minor: caps.get(2)?.as_str().parse().ok()?,
        patch: caps
            .get(3)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0),
    })
}

fn compatibility_score(version: Version, hint: Option<VersionHint>) -> i64 {
    let major_score = if version.major == 4 { 0 } else { 100_000 };
    let minor_score = match hint {
        Some(VersionHint {
            major,
            minor: Some(minor),
        }) if version.major == major => (version.minor as i64 - minor as i64).abs(),
        Some(VersionHint { major, minor: None }) if version.major == major => 50,
        _ => 100,
    };
    let patch_score = version.patch as i64;
    major_score + minor_score * 100 + patch_score
}

#[cfg(test)]
mod tests {
    use super::{
        choose_candidate, compatibility_score, parse_version_output, project_version_hint,
        Candidate, ProbedCandidate, Version, VersionHint,
    };
    use std::ffi::OsString;
    use std::fs;

    #[test]
    fn parses_project_version_hint() {
        let dir = std::env::temp_dir().join(format!("gdtest-project-hint-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.godot"),
            "[application]\nconfig/features=PackedStringArray(\"4.6\", \"GL Compatibility\")\n",
        )
        .unwrap();
        let hint = project_version_hint(&dir).unwrap();
        assert_eq!(hint.major, 4);
        assert_eq!(hint.minor, Some(6));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn parses_godot_version_output() {
        let version = parse_version_output("Godot Engine v4.6.stable.official.123456").unwrap();
        assert_eq!(version.major, 4);
        assert_eq!(version.minor, 6);
    }

    #[test]
    fn scoring_prefers_exact_minor() {
        let hint = Some(VersionHint {
            major: 4,
            minor: Some(6),
        });
        let exact = compatibility_score(
            Version {
                major: 4,
                minor: 6,
                patch: 0,
            },
            hint,
        );
        let near = compatibility_score(
            Version {
                major: 4,
                minor: 5,
                patch: 0,
            },
            hint,
        );
        let incompatible = compatibility_score(
            Version {
                major: 3,
                minor: 5,
                patch: 0,
            },
            hint,
        );
        assert!(exact < near);
        assert!(near < incompatible);
    }

    #[test]
    fn chooser_prefers_exact_matching_candidate() {
        let result = choose_candidate(
            vec![
                probed("godot-4.5", Some((4, 5, 0))),
                probed("godot-4.6", Some((4, 6, 0))),
                probed("godot-3.5", Some((3, 5, 0))),
            ],
            Some(VersionHint {
                major: 4,
                minor: Some(6),
            }),
        )
        .unwrap();
        assert_eq!(result.path, "godot-4.6");
        assert!(result.warning.is_none());
    }

    #[test]
    fn chooser_uses_unverified_candidate_as_last_resort() {
        let result = choose_candidate(
            vec![probed("godot", None)],
            Some(VersionHint {
                major: 4,
                minor: Some(6),
            }),
        )
        .unwrap();
        assert_eq!(result.path, "godot");
        assert!(result.warning.unwrap().contains("unverified"));
    }

    #[test]
    fn chooser_errors_when_only_incompatible_candidates_exist() {
        let err = choose_candidate(
            vec![
                probed("godot-3.5", Some((3, 5, 0))),
                probed("godot-2.1", Some((2, 1, 0))),
            ],
            Some(VersionHint {
                major: 4,
                minor: Some(6),
            }),
        )
        .unwrap_err();
        let text = err.to_string();
        assert!(text.contains("could not find a compatible Godot 4 executable"));
        assert!(text.contains("godot-3.5 (3.5.0)"));
    }

    fn probed(name: &str, version: Option<(u32, u32, u32)>) -> ProbedCandidate {
        ProbedCandidate {
            candidate: Candidate {
                command: OsString::from(name),
                display: name.to_string(),
            },
            version: version.map(|(major, minor, patch)| Version {
                major,
                minor,
                patch,
            }),
        }
    }
}
