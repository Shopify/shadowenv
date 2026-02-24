use crate::cli::InitCmd::{self, *};
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(cmd: InitCmd) -> Result<()> {
    // Attempt to resolve the path which was passed to argv[0]
    // This is to avoid the situation where fully resolving to something like a nix
    // store path could be garbage collected while the shell hook is still used
    let exe = std::env::args().next().unwrap();
    let pb = if Path::new(&exe).is_absolute() {
        PathBuf::from(exe)
    } else {
        std::env::current_dir().unwrap().join(exe)
    };
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
