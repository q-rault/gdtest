use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy)]
struct Asset {
    rel_path: &'static str,
    contents: &'static str,
}

macro_rules! asset {
    ($path:literal) => {
        Asset {
            rel_path: $path,
            contents: include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/", $path)),
        }
    };
}

const RUNTIME_ASSETS: &[Asset] = &[
    asset!("addons/gdtest/LICENSE"),
    asset!("addons/gdtest/README.md"),
    asset!("addons/gdtest/gd_test.gd"),
    asset!("addons/gdtest/gd_test.gd.uid"),
    asset!("addons/gdtest/gdt.gd"),
    asset!("addons/gdtest/gdt.gd.uid"),
    asset!("addons/gdtest/gdtest_worker.gd"),
    asset!("addons/gdtest/gdtest_worker.gd.uid"),
    asset!("addons/gdtest/gdunit4_chain.gd"),
    asset!("addons/gdtest/gdunit4_chain.gd.uid"),
    asset!("addons/gdtest/gdunit4_compat.gd"),
    asset!("addons/gdtest/gdunit4_compat.gd.uid"),
    asset!("addons/gdtest/scene_runner.gd"),
    asset!("addons/gdtest/scene_runner.gd.uid"),
];

const EDITOR_ASSETS: &[Asset] = &[
    asset!("addons/gdtest_editor/LICENSE"),
    asset!("addons/gdtest_editor/README.md"),
    asset!("addons/gdtest_editor/plugin.cfg"),
    asset!("addons/gdtest_editor/plugin.gd"),
    asset!("addons/gdtest_editor/plugin.gd.uid"),
    asset!("addons/gdtest_editor/test_panel.gd"),
    asset!("addons/gdtest_editor/test_panel.gd.uid"),
];

/// The addon files gdtest ships, as `(path in the project, contents)`: the
/// runtime harness under `addons/gdtest/` and the editor panel under
/// `addons/gdtest_editor/`.
pub fn addon_files() -> impl Iterator<Item = (&'static str, &'static str)> {
    RUNTIME_ASSETS
        .iter()
        .chain(EDITOR_ASSETS)
        .map(|asset| (asset.rel_path, asset.contents))
}

pub struct InitOptions {
    pub project_path: String,
    pub editor: bool,
    pub force: bool,
}

pub fn run(opts: InitOptions) -> Result<()> {
    let project_abs = PathBuf::from(&opts.project_path)
        .canonicalize()
        .with_context(|| format!("project path does not exist: {}", opts.project_path))?;
    ensure_project_root(&project_abs)?;

    let mut wrote = 0usize;
    wrote += install_assets(&project_abs, RUNTIME_ASSETS, opts.force)?;
    if opts.editor {
        wrote += install_assets(&project_abs, EDITOR_ASSETS, opts.force)?;
    }

    eprintln!(
        "gdtest init: installed runtime addon into {}",
        project_abs.join("addons").display()
    );
    if opts.editor {
        eprintln!("gdtest init: installed Godot editor plugin");
        eprintln!("gdtest init: enable it in Project -> Project Settings -> Plugins");
    } else {
        eprintln!("gdtest init: skipped Godot editor plugin (--no-editor)");
    }
    eprintln!(
        "gdtest init: IDE extensions are separate: see vscode-gdtest/ and zed-gdtest/ in the repository"
    );
    if wrote == 0 {
        eprintln!("gdtest init: nothing changed");
    }
    Ok(())
}

fn ensure_project_root(project_abs: &Path) -> Result<()> {
    let project_file = project_abs.join("project.godot");
    if !project_file.is_file() {
        bail!(
            "project path is not a Godot project root (missing {})",
            project_file.display()
        );
    }
    Ok(())
}

fn install_assets(project_abs: &Path, assets: &[Asset], force: bool) -> Result<usize> {
    let mut wrote = 0usize;
    for asset in assets {
        let target = project_abs.join(asset.rel_path);
        if write_asset(&target, asset.contents, force)? {
            wrote += 1;
        }
    }
    Ok(wrote)
}

fn write_asset(target: &Path, contents: &str, force: bool) -> Result<bool> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("creating directory {}", parent.display()))?;
    }

    if target.exists() {
        let current =
            fs::read_to_string(target).with_context(|| format!("reading {}", target.display()))?;
        if current == contents {
            return Ok(false);
        }
        if !force {
            bail!(
                "refusing to overwrite modified file {} (rerun with --force)",
                target.display()
            );
        }
    }

    fs::write(target, contents).with_context(|| format!("writing {}", target.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{InitOptions, run};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gdtest-init-{name}-{nonce}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.godot"),
            "[application]\nconfig/name=\"tmp\"\n",
        )
        .unwrap();
        dir
    }

    #[test]
    fn installs_runtime_addon_and_editor_by_default() {
        let dir = temp_project("default");
        run(InitOptions {
            project_path: dir.display().to_string(),
            editor: true,
            force: false,
        })
        .unwrap();

        assert!(dir.join("addons/gdtest/gdtest_worker.gd").is_file());
        assert!(dir.join("addons/gdtest/gdt.gd").is_file());
        assert!(dir.join("addons/gdtest_editor/plugin.cfg").is_file());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn skips_editor_when_disabled() {
        let dir = temp_project("no-editor");
        run(InitOptions {
            project_path: dir.display().to_string(),
            editor: false,
            force: false,
        })
        .unwrap();

        assert!(!dir.join("addons/gdtest_editor/plugin.cfg").exists());
        assert!(!dir.join("addons/gdtest_editor/plugin.gd").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    /// `assets/addons` is what ships to crates.io; `addons/` is what the
    /// self-tests exercise. Run `just sync-assets` when this fails.
    #[test]
    fn embedded_assets_match_the_repo_addons() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let addons = repo_root.join("addons");
        if !addons.is_dir() {
            return; // Built from the published crate, which has no repo root.
        }
        for asset in super::RUNTIME_ASSETS.iter().chain(super::EDITOR_ASSETS) {
            let on_disk = fs::read_to_string(repo_root.join(asset.rel_path)).unwrap();
            assert_eq!(
                on_disk.replace("\r\n", "\n"),
                asset.contents.replace("\r\n", "\n"),
                "{} is out of date; run `just sync-assets`",
                asset.rel_path
            );
        }
    }

    #[test]
    fn refuses_to_overwrite_modified_file_without_force() {
        let dir = temp_project("force");
        let runtime_dir = dir.join("addons/gdtest");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::write(runtime_dir.join("gdt.gd"), "modified").unwrap();

        let err = run(InitOptions {
            project_path: dir.display().to_string(),
            editor: false,
            force: false,
        })
        .unwrap_err();
        assert!(
            err.to_string().contains("rerun with --force"),
            "unexpected error: {err:#}"
        );

        run(InitOptions {
            project_path: dir.display().to_string(),
            editor: false,
            force: true,
        })
        .unwrap();
        let contents = fs::read_to_string(runtime_dir.join("gdt.gd")).unwrap();
        assert!(contents.contains("class_name Gdt"));

        fs::remove_dir_all(dir).unwrap();
    }
}
