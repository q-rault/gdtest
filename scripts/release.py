import pathlib
import re
import subprocess
import sys


SEMVER_RE = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*)?")
ROOT = pathlib.Path(__file__).resolve().parent.parent


def read(path: str) -> str:
    return (ROOT / path).read_text()


def replace(path: str, pattern: str, repl: str, *, flags: int = re.MULTILINE, count: int = 1) -> None:
    file_path = ROOT / path
    text, changed = re.subn(pattern, repl, file_path.read_text(), count=count, flags=flags)
    if changed < 1:
        raise SystemExit(f"ERROR: pattern not found in {path}")
    file_path.write_text(text)
    print(f"  {path}")


def check_clean_git() -> None:
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    if result.stdout.strip():
        sys.stderr.write("ERROR: working tree must be clean before release\n")
        sys.stderr.write(result.stdout)
        raise SystemExit(1)


def check_release_version(version: str) -> None:
    if not SEMVER_RE.fullmatch(version):
        raise SystemExit(
            f"ERROR: invalid release version '{version}'. Expected semver like 0.2.0 or 0.2.0-beta.1"
        )


def cargo_version() -> str:
    match = re.search(r'^\s*version\s*=\s*"([^"]+)"', read("Cargo.toml"), re.MULTILINE)
    if match is None:
        raise SystemExit("ERROR: failed to read version from Cargo.toml")
    return match.group(1)


def check_version_sync() -> None:
    expected = cargo_version()
    cargo_text = read("Cargo.toml")
    checks = [
        ("Cargo.toml#gdtest-analysis", re.search(r'^\s*gdtest-analysis\s*=\s*\{[^}]*version\s*=\s*"([^"]+)"', cargo_text, re.MULTILINE)),
        ("vscode-gdtest/package.json", re.search(r'"version"\s*:\s*"([^"]+)"', read("vscode-gdtest/package.json"))),
        ("vscode-gdtest/package-lock.json", re.search(r'"version"\s*:\s*"([^"]+)"', read("vscode-gdtest/package-lock.json"))),
        (
            "vscode-gdtest/package-lock.json#root",
            re.search(
                r'""\s*:\s*\{\s*"name"\s*:\s*"vscode-gdtest",\s*"version"\s*:\s*"([^"]+)"',
                read("vscode-gdtest/package-lock.json"),
                re.MULTILINE | re.DOTALL,
            ),
        ),
        ("zed-gdtest/extension.toml", re.search(r'^\s*version\s*=\s*"([^"]+)"', read("zed-gdtest/extension.toml"), re.MULTILINE)),
        ("addons/gdtest_editor/plugin.cfg", re.search(r'^\s*version\s*=\s*"([^"]+)"', read("addons/gdtest_editor/plugin.cfg"), re.MULTILINE)),
    ]

    mismatches = [path for path, match in checks if match is None or match.group(1) != expected]
    if mismatches:
        sys.stderr.write(f"ERROR: version mismatch against Cargo.toml {expected}\n")
        for path in mismatches:
            sys.stderr.write(f"  {path}\n")
        raise SystemExit(1)


def bump(version: str) -> None:
    check_release_version(version)
    print(f"bumping to {version}")
    replace("Cargo.toml", r'^(version\s*=\s*)"[^"]+"', f'\\g<1>"{version}"')
    replace("Cargo.toml", r'^(gdtest-analysis\s*=\s*\{[^}]*version\s*=\s*)"[^"]+"', f'\\g<1>"{version}"')
    replace("vscode-gdtest/package.json", r'("version"\s*:\s*)"[^"]+"', f'\\g<1>"{version}"')
    replace("vscode-gdtest/package-lock.json", r'("version"\s*:\s*)"[^"]+"', f'\\g<1>"{version}"')
    replace(
        "vscode-gdtest/package-lock.json",
        r'(""\s*:\s*\{\s*"name"\s*:\s*"vscode-gdtest",\s*"version"\s*:\s*)"[^"]+"',
        f'\\g<1>"{version}"',
        flags=re.MULTILINE | re.DOTALL,
    )
    replace("zed-gdtest/extension.toml", r'^(version\s*=\s*)"[^"]+"', f'\\g<1>"{version}"')
    replace("addons/gdtest_editor/plugin.cfg", r'^(version\s*=\s*)"[^"]+"', f'\\g<1>"{version}"')
    print("done - run `just check-version-sync` before releasing")


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        raise SystemExit("usage: python scripts/release.py <command> [version]")
    command = argv[1]
    if command == "check-clean-git":
        check_clean_git()
    elif command == "check-release-version":
        check_release_version(argv[2])
    elif command == "check-version-sync":
        check_version_sync()
    elif command == "bump":
        bump(argv[2])
    else:
        raise SystemExit(f"unknown command: {command}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
