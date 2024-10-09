use crate::{features::Feature, loader, trust};
use anyhow::{anyhow, Error};
use regex::Regex;
use std::{
    collections::HashSet,
    env,
    fs::{self, OpenOptions},
    io::IsTerminal,
    path::PathBuf,
    time::{Duration, SystemTime},
};

// "shadowenv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const SHADOWENV: &'static str = concat!(
    "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo",
    "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m",
);

const COOLDOWN_SECONDS: u64 = 5;

fn cooldown() -> Duration {
    Duration::new(COOLDOWN_SECONDS, 0)
}

pub fn handle_hook_error(err: Error, shellpid: u32, silent: bool) -> i32 {
    if silent {
        return 1;
    }

    if let Ok(true) = check_and_trigger_cooldown(&err, shellpid) {
        return 1;
    };
    let err = backticks_to_bright_green(err);
    eprintln!("{} \x1b[1;31mfailure: {}\x1b[0m", SHADOWENV, err);
    return 1;
}

pub fn print_activation_to_tty(activated: bool, features: HashSet<Feature>) {
    if !should_print_activation() {
        return;
    }
    if activated {
        if features.len() == 0 {
            eprint!("\x1b[1;34mactivated {}", SHADOWENV);
        } else {
            let feature_list = features
                .iter()
                .map(|s| format!("{}", s))
                .collect::<Vec<String>>()
                .join(", ");
            eprint!(
                "\x1b[1;34mactivated {} \x1b[1;34m({})",
                SHADOWENV, feature_list
            );
        }
    } else {
        eprint!("\x1b[1;34mdeactivated {}\x1b[1;34m", SHADOWENV);
    }
    eprintln!("\x1b[0m");
}

fn backticks_to_bright_green(err: Error) -> String {
    let re = Regex::new(r"`(.*?)`").unwrap();
    // this is almost certainly not the best way to do this, but this runs at most once per
    // execution so I only care so much.
    let before = format!("{}", err);
    re.replace_all(before.as_ref(), "\x1b[1;32m$1\x1b[1;31m")
        .to_string()
}

fn check_and_trigger_cooldown(err: &Error, shellpid: u32) -> Result<bool, Error> {
    // if no .shadowenv.d, then Err(_) just means no cooldown: always display error.
    let root = loader::find_root(&env::current_dir()?, loader::DEFAULT_RELATIVE_COMPONENT)?
        .ok_or_else(|| anyhow!("no .shadowenv.d"))?;

    let _ = clean_up_stale_errors(&root, Duration::new(300, 0));

    let errindex = cooldown_index(err).ok_or_else(|| anyhow!("error not subject to cooldown"))?;

    let errfilepath = err_file(&root, errindex, shellpid)?;

    match check_cooldown_sentinel(&errfilepath, cooldown()) {
        Ok(true) => Ok(true),
        _ => {
            create_cooldown_sentinel(errfilepath)?;
            Ok(false)
        }
    }
}

fn cooldown_index(err: &Error) -> Option<u32> {
    match err.downcast_ref::<trust::NotTrusted>() {
        Some(_) => Some(0),
        None => None,
    }
}

fn clean_up_stale_errors(root: &PathBuf, timeout: Duration) -> Result<(), Error> {
    let now = SystemTime::now();
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().starts_with(".error-") {
                continue;
            }
            if let Ok(mtime) = entry.metadata().and_then(|md| md.modified()) {
                if let Ok(duration) = now.duration_since(mtime) {
                    if duration > timeout {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
    Ok(())
}

fn err_file(root: &PathBuf, errindex: u32, shellpid: u32) -> Result<PathBuf, Error> {
    Ok(root.join(format!(".error-{}-{}", errindex, shellpid)))
}

// return value of Ok(true) indicates it's on cooldown and should be suppressed.
fn check_cooldown_sentinel(path: &PathBuf, timeout: Duration) -> Result<bool, Error> {
    let metadata = path.metadata()?;
    let mtime = metadata.modified()?;

    let now = SystemTime::now();
    let elapsed = now.duration_since(mtime)?;

    Ok(elapsed < timeout)
}

fn create_cooldown_sentinel(path: PathBuf) -> Result<(), Error> {
    let _ = OpenOptions::new()
        .truncate(true)
        .write(true)
        .create(true)
        .open(path)?;
    Ok(())
}

fn should_print_activation() -> bool {
    let configured_to_print: bool;
    match env::var("SHADOWENV_SILENT") {
        Ok(value) => match value.to_lowercase().as_str() {
            "0" | "false" | "no" | "" => configured_to_print = true,
            _ => configured_to_print = false,
        },
        Err(_) => configured_to_print = true,
    };

    return std::io::stderr().is_terminal() && configured_to_print;
}
