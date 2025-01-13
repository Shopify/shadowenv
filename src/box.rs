use crate::{cli::BoxCmd, get_current_dir_or_exit, hook, shadowenv::Shadowenv};
use anyhow::{anyhow, Error};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::{iter, path::PathBuf};

/// Boxute the provided command (argv) after loading the environment from the current directory
pub fn run(cmd: BoxCmd) -> Result<(), Error> {
    let data = Shadowenv::from_env();
    let pathbuf = cmd
        .dir
        .map(|d| PathBuf::from(d))
        .unwrap_or(get_current_dir_or_exit());

    let shadowenv = match hook::load_env(pathbuf.clone(), data, true) {
        Ok(Some(shadowenv)) => shadowenv,
        Ok(None) => return Err(anyhow!("No .shadowenv.d found in {}", pathbuf.display())),
        Err(err) => return Err(err),
    };

    hook::mutate_own_env(&shadowenv)?;
    build_box(pathbuf.clone(), &shadowenv)?;

    let mut argv = vec![
        "-f".to_string(),
        profile_file(pathbuf, &shadowenv)
            .to_string_lossy()
            .to_string(),
    ];
    argv.extend(iter::once(cmd.cmd).chain(cmd.cmd_argv).collect::<Vec<_>>());

    // exec only returns if it was unable to start the new process, and it's always an error.
    let err = exec::Command::new("sandbox-exec").args(&argv).exec();
    Err(err.into())
}

fn build_box(pathbuf: PathBuf, shadowenv: &Shadowenv) -> Result<(), Error> {
    let profile_path = profile_file(pathbuf, shadowenv);

    let mut profile = "(version 1)\n".to_string();

    for (operation, path) in shadowenv.operations() {
        match operation.as_str() {
            "deny" => {
                profile.push_str(&format!(
                    "(deny file-read* file-write* (subpath {}))\n",
                    path
                ));
                continue;
            }
            "allow-ro" => {
                profile.push_str(&format!("(allow file-read* (subpath {}))\n", path));
                continue;
            }
            "allow-rw" => {
                profile.push_str(&format!(
                    "(allow file-read* file-write* (subpath {}))\n",
                    path
                ));
                continue;
            }
            _ => {
                return Err(anyhow!("Unknown operation: {}", operation));
            }
        }
    }

    std::fs::write(profile_path, profile)?;

    Ok(())
}

fn profile_file(root: PathBuf, shadowenv: &Shadowenv) -> PathBuf {
    root.join(format!(".profile-{}", fingerprint(shadowenv)))
}

fn fingerprint(shadowenv: &Shadowenv) -> String {
    calculate_hash(&shadowenv.operations()).to_string()
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}
