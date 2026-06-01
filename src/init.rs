use crate::cli::InitCmd::{self, *};
use anyhow::{anyhow, Context, Result};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(cmd: InitCmd) -> Result<()> {
    // Resolve argv[0] to an absolute path so the generated shell hook can invoke
    // shadowenv from any working directory.
    let exe = std::env::args().next().unwrap();
    let pb = resolve_self_path(&exe);
    match cmd {
        Bash(opts) => print_script(
            pb,
            include_bytes!("../sh/shadowenv.bash.in"),
            opts.no_hookbook,
        ),
        Zsh(opts) => print_script(
            pb,
            include_bytes!("../sh/shadowenv.zsh.in"),
            opts.no_hookbook,
        ),
        Fish => print_script(
            pb,
            include_bytes!("../sh/shadowenv.fish.in"),
            true, // Fish doesn't use hookbook
        ),
        Nushell => install_nushell_hook(pb),
    }
}

/// Resolve the path passed as argv[0] to an absolute path to the shadowenv
/// executable, suitable for embedding in a shell hook.
///
/// We deliberately avoid [`std::env::current_exe`] for the common cases because
/// it canonicalizes symlinks. Resolving to something like a Nix store path could
/// later be garbage collected while the shell hook is still in use, so we prefer
/// a stable path (e.g. a package-manager-managed symlink found on `$PATH`).
fn resolve_self_path(exe: &str) -> PathBuf {
    resolve_self_path_inner(
        exe,
        std::env::current_dir().ok(),
        std::env::var_os("PATH"),
        || std::env::current_exe().ok(),
    )
}

fn resolve_self_path_inner(
    exe: &str,
    cwd: Option<PathBuf>,
    path_var: Option<impl AsRef<OsStr>>,
    current_exe: impl Fn() -> Option<PathBuf>,
) -> PathBuf {
    let path = Path::new(exe);
    if path.is_absolute() {
        // Already absolute, e.g. invoked via `/usr/local/bin/shadowenv`.
        return path.to_path_buf();
    }

    if exe.contains(std::path::MAIN_SEPARATOR) {
        // A relative path containing a separator (e.g. `./shadowenv` or
        // `bin/shadowenv`) is resolved against the current directory.
        if let Some(cwd) = &cwd {
            return cwd.join(exe);
        }
    } else if let Some(found) = path_var.and_then(|p| find_in_path(exe, p.as_ref())) {
        // A bare command name (no separator) was located by the shell via
        // `$PATH`. Search `$PATH` ourselves to recover an absolute path rather
        // than incorrectly joining the name with the current directory.
        return found;
    }

    // Fall back to the canonical executable path. This may resolve symlinks, but
    // a correct absolute path is better than one that does not exist.
    current_exe()
        .or_else(|| cwd.map(|cwd| cwd.join(exe)))
        .unwrap_or_else(|| PathBuf::from(exe))
}

/// Search the directories in `$PATH` for an executable file named `name`,
/// returning the first match. Symlinks are intentionally not resolved.
fn find_in_path(name: &str, path_var: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path_var).find_map(|dir| {
        if dir.as_os_str().is_empty() {
            return None;
        }
        let candidate = dir.join(name);
        is_executable(&candidate).then_some(candidate)
    })
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn install_nushell_hook(selfpath: PathBuf) -> Result<()> {
    let output = Command::new("nu")
        .args(["-c", "$nu.user-autoload-dirs | first"])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                anyhow!("Could not find 'nu' in PATH. Please ensure nushell is installed.")
            } else {
                anyhow!("Failed to run 'nu': {}", e)
            }
        })?;

    if !output.status.success() {
        return Err(anyhow!(
            "Failed to query nushell autoload directory: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let autoload_dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let autoload_path = PathBuf::from(&autoload_dir);

    fs::create_dir_all(&autoload_path)
        .with_context(|| format!("Failed to create autoload directory '{}'", autoload_dir))?;

    let script_path = autoload_path.join("shadowenv.nu");
    let script = String::from_utf8_lossy(include_bytes!("../sh/shadowenv.nushell.in"));
    let script = script.replace("@SELF@", selfpath.to_str().unwrap());

    fs::write(&script_path, script.as_bytes())
        .with_context(|| format!("Failed to write '{}'", script_path.display()))?;

    println!("Wrote shadowenv hook to {}", script_path.display());
    Ok(())
}

fn print_script(selfpath: PathBuf, bytes: &[u8], no_hookbook: bool) -> Result<()> {
    let script = String::from_utf8_lossy(bytes);
    let script = script.replace("@SELF@", selfpath.into_os_string().to_str().unwrap());

    if no_hookbook {
        // If no_hookbook is true, replace @HOOKBOOK@ with an empty string
        let script = script.replace("@HOOKBOOK@", "");
        println!("{}", script);
    } else {
        // Otherwise, include the hookbook as before, but pad with newlines
        let hookbook = String::from_utf8_lossy(include_bytes!("../sh/hookbook.sh"));
        let padded_hookbook = format!("\n{}\n", hookbook);
        let script = script.replace("@HOOKBOOK@", &padded_hookbook);
        println!("{}", script);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    const FAKE_CURRENT_EXE: &str = "/canonical/path/shadowenv";

    fn current_exe() -> Option<PathBuf> {
        Some(PathBuf::from(FAKE_CURRENT_EXE))
    }

    #[test]
    fn absolute_argv0_is_used_verbatim() {
        let got = resolve_self_path_inner(
            "/usr/local/bin/shadowenv",
            Some(PathBuf::from("/some/cwd")),
            Some(OsString::from("/usr/local/bin")),
            current_exe,
        );
        assert_eq!(got, PathBuf::from("/usr/local/bin/shadowenv"));
    }

    #[test]
    fn relative_argv0_with_separator_is_joined_with_cwd() {
        let got = resolve_self_path_inner(
            "./shadowenv",
            Some(PathBuf::from("/work/dir")),
            Some(OsString::from("/usr/bin")),
            current_exe,
        );
        assert_eq!(got, PathBuf::from("/work/dir/./shadowenv"));
    }

    #[test]
    fn bare_argv0_is_resolved_via_path() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("shadowenv");
        fs::write(&exe, b"#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
        }

        let got = resolve_self_path_inner(
            "shadowenv",
            // A bare name must NOT be resolved against the current directory.
            Some(PathBuf::from("/should/not/be/used")),
            Some(OsString::from(dir.path())),
            current_exe,
        );
        assert_eq!(got, exe);
    }

    #[test]
    fn bare_argv0_never_joins_cwd() {
        // Regression test for the `$PWD/shadowenv` bug: a bare command name that
        // cannot be found on $PATH must fall back to the canonical exe path,
        // never to `<cwd>/shadowenv`.
        let cwd = PathBuf::from("/Users/someone/project");
        let got = resolve_self_path_inner(
            "shadowenv",
            Some(cwd.clone()),
            Some(OsString::from("/nonexistent-dir-abc")),
            current_exe,
        );
        assert_ne!(got, cwd.join("shadowenv"));
        assert_eq!(got, PathBuf::from(FAKE_CURRENT_EXE));
    }

    #[cfg(unix)]
    #[test]
    fn find_in_path_skips_empty_entries_and_non_executables() {
        let dir = tempfile::tempdir().unwrap();
        // A non-executable file with the right name should be skipped.
        let non_exec = dir.path().join("shadowenv");
        fs::write(&non_exec, b"not executable\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&non_exec, fs::Permissions::from_mode(0o644)).unwrap();
        }

        // PATH with a leading empty entry (":") and the dir holding the
        // non-executable file: nothing should match.
        let path_var = OsString::from(format!(":{}", dir.path().display()));
        assert_eq!(find_in_path("shadowenv", &path_var), None);
    }
}
