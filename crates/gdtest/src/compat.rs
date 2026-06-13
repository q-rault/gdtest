//! Version compatibility checks: harness ↔ CLI and Godot version range.

pub const GODOT_MIN: (u32, u32, u32) = (4, 0, 0);
pub const GODOT_MAX_MAJOR_EXCLUSIVE: u32 = 5;

const CLI_VERSION: &str = env!("CARGO_PKG_VERSION");

fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.splitn(4, '.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next()?.parse().ok()?;
    let patch: u32 = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
    Some((major, minor, patch))
}

/// Validate the harness version reported in the `ready` event against the CLI version.
/// Requires matching major.minor. A missing version triggers a warning rather than an error
/// to stay backward-compatible with harnesses that predate this check.
pub fn check_harness_version(reported: Option<&str>) -> anyhow::Result<()> {
    let Some(v) = reported else {
        eprintln!(
            "gdtest: warning: harness version unknown — addons/gdtest/ may be outdated. \
             Update it to match CLI version {CLI_VERSION}."
        );
        return Ok(());
    };
    let (Some((h_major, h_minor, _)), Some((c_major, c_minor, _))) =
        (parse_version(v), parse_version(CLI_VERSION))
    else {
        return Ok(());
    };
    if h_major != c_major || h_minor != c_minor {
        anyhow::bail!(
            "Harness version {v} is incompatible with this CLI ({CLI_VERSION}). \
             Update addons/gdtest/ to match your CLI version."
        );
    }
    Ok(())
}

/// Validate the Godot version reported by the harness against the declared compatible range.
/// A missing version (old harness) is silently skipped.
pub fn check_godot_version(reported: Option<&str>) -> anyhow::Result<()> {
    let Some(v) = reported else {
        return Ok(());
    };
    let Some((major, minor, patch)) = parse_version(v) else {
        return Ok(());
    };
    let ok = (major, minor, patch) >= GODOT_MIN && major < GODOT_MAX_MAJOR_EXCLUSIVE;
    if !ok {
        anyhow::bail!(
            "Godot {v} is not supported. gdtest requires Godot {}.{}.{}–{}.x.",
            GODOT_MIN.0,
            GODOT_MIN.1,
            GODOT_MIN.2,
            GODOT_MAX_MAJOR_EXCLUSIVE - 1
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn godot_version_in_range() {
        assert!(check_godot_version(Some("4.3.0")).is_ok());
        assert!(check_godot_version(Some("4.0.0")).is_ok());
        assert!(check_godot_version(None).is_ok());
    }

    #[test]
    fn godot_version_out_of_range() {
        assert!(check_godot_version(Some("3.5.3")).is_err());
        assert!(check_godot_version(Some("5.0.0")).is_err());
    }

    #[test]
    fn harness_version_missing_warns_not_errors() {
        assert!(check_harness_version(None).is_ok());
    }

    #[test]
    fn harness_version_matches() {
        assert!(check_harness_version(Some(CLI_VERSION)).is_ok());
    }
}
