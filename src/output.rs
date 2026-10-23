use crate::{features::Feature, lang, loader, trust};
use anyhow::{anyhow, Error};
use regex::Regex;
use std::{
    borrow::Cow,
    collections::HashSet,
    env,
    fmt::Write,
    fs::{self, OpenOptions},
    io::IsTerminal,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// Escapes characters a terminal would act on rather than display: control characters other
/// than newline and tab (ESC, and the C1 controls that also begin escape sequences) and the
/// Unicode bidirectional overrides. Text shown to the user can come from untrusted places:
/// directory names, `parent` symlink targets, `.lisp` source, `provide` arguments, values.
pub fn sanitize_for_terminal(s: &str) -> Cow<'_, str> {
    let acted_on = |c: char| {
        (c.is_control() && c != '\n' && c != '\t')
            || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
    };
    if !s.chars().any(acted_on) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        if acted_on(c) {
            let _ = write!(out, "\\u{{{:x}}}", c as u32);
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

// "shadowenv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const SHADOWENV: &str = concat!(
    "\x1b[38;5;249m░",
    "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo",
    "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m",
);

const COOLDOWN_SECONDS: u64 = 5;

fn cooldown() -> Duration {
    Duration::from_secs(COOLDOWN_SECONDS)
}

pub fn format_hook_error(err: Error, shellpid: u32, silent: bool) -> Option<String> {
    if silent || matches!(check_and_trigger_cooldown(&err, shellpid), Ok(true)) {
        return None;
    }

    // A Shadowlisp error's detail goes on its own lines ahead of the banner, as under Ketos:
    // dev reads the start of stderr to find a generated file that doesn't parse.
    if let Some(lisp) = err.downcast_ref::<lang::Error>() {
        let detail = sanitize_for_terminal(&lisp.detail()).into_owned();
        return Some(format!(
            "{detail}\n{SHADOWENV} \x1b[1;31mfailure: error while evaluating shadowlisp\x1b[0m"
        ));
    }
    let err = backticks_to_bright_green(err);
    Some(format!("{} \x1b[1;31mfailure: {}\x1b[0m", SHADOWENV, err))
}

pub fn print_activation_to_tty(
    current_dirs: HashSet<PathBuf>,
    prev_dirs: HashSet<PathBuf>,
    features: HashSet<Feature>,
) {
    if !should_print_activation() {
        return;
    }
    let added_dirs: HashSet<PathBuf> = current_dirs.difference(&prev_dirs).cloned().collect();
    let removed_dirs: HashSet<PathBuf> = prev_dirs.difference(&current_dirs).cloned().collect();

    let feature_list = if !features.is_empty() {
        format!(
            " \x1b[1;38;5;245m{}",
            features
                .iter()
                .map(|s| sanitize_for_terminal(&s.to_string()).into_owned())
                .collect::<Vec<_>>()
                .join("\x1b[38;5;240m,\x1b[1;38;5;245m")
        )
    } else {
        String::new()
    };

    eprintln!(
        "\x1b[1;34m{}{}{}\x1b[0m",
        SHADOWENV,
        dir_diff(added_dirs, removed_dirs).unwrap_or_default(),
        feature_list
    );
}

fn dir_diff(added_dirs: HashSet<PathBuf>, removed_dirs: HashSet<PathBuf>) -> Option<String> {
    if added_dirs.is_empty() && removed_dirs.is_empty() {
        return None;
    }

    let mut output = String::with_capacity(64);
    output.push_str("\x1b[38;5;240m[");

    if !added_dirs.is_empty() {
        output.push_str("\x1b[0;32m");
        output.push_str(&"+".repeat(added_dirs.len()));
    }

    if !added_dirs.is_empty() && !removed_dirs.is_empty() {
        output.push_str("\x1b[38;5;240m|");
    }

    if !removed_dirs.is_empty() {
        output.push_str("\x1b[0;31m");
        output.push_str(&"-".repeat(removed_dirs.len()));
    }

    output.push_str("\x1b[38;5;240m]\x1b[0m");
    Some(output)
}

fn backticks_to_bright_green(err: Error) -> String {
    let re = Regex::new(r"`(.*?)`").unwrap();
    // this is almost certainly not the best way to do this, but this runs at most once per
    // execution so I only care so much.
    let before = sanitize_for_terminal(&err.to_string()).into_owned();
    re.replace_all(before.as_ref(), "\x1b[1;32m$1\x1b[1;31m")
        .to_string()
}

fn check_and_trigger_cooldown(err: &Error, shellpid: u32) -> Result<bool, Error> {
    // if no .shadowenv.d, then Err(_) just means no cooldown: always display error.
    let roots = loader::find_shadowenv_paths(&env::current_dir()?)?;
    if roots.is_empty() {
        return Err(anyhow!("no .shadowenv.d"));
    }

    let root = roots.first().unwrap();

    let _ = clean_up_stale_errors(root, Duration::new(300, 0));

    let errindex = cooldown_index(err).ok_or_else(|| anyhow!("error not subject to cooldown"))?;

    let errfilepath = err_file(root, errindex, shellpid)?;

    match check_cooldown_sentinel(&errfilepath, cooldown()) {
        Ok(true) => Ok(true),
        _ => {
            create_cooldown_sentinel(errfilepath)?;
            Ok(false)
        }
    }
}

fn cooldown_index(err: &Error) -> Option<u32> {
    err.downcast_ref::<trust::NotTrusted>().map(|_| 0)
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

fn err_file(root: &Path, errindex: u32, shellpid: u32) -> Result<PathBuf, Error> {
    Ok(root.join(format!(".error-{}-{}", errindex, shellpid)))
}

// return value of Ok(true) indicates it's on cooldown and should be suppressed.
//
// The sentinel lives in a `.shadowenv.d` that isn't trusted yet, so its contents are the
// attacker's: never follow a symlink there.
fn check_cooldown_sentinel(path: &Path, timeout: Duration) -> Result<bool, Error> {
    let metadata = path.symlink_metadata()?;
    if !metadata.is_file() {
        return Ok(false);
    }
    let mtime = metadata.modified()?;

    let now = SystemTime::now();
    let elapsed = now.duration_since(mtime)?;

    Ok(elapsed < timeout)
}

/// Creates the sentinel afresh. `remove_file` deletes a symlink itself, not its target, and
/// `create_new` (`O_CREAT | O_EXCL`) refuses to follow one, so a planted link can't redirect
/// the write to another file.
fn create_cooldown_sentinel(path: PathBuf) -> Result<(), Error> {
    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    OpenOptions::new().write(true).create_new(true).open(path)?;
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

    std::io::stderr().is_terminal() && configured_to_print
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_for_terminal_escapes_what_terminals_act_on() {
        assert_eq!(
            sanitize_for_terminal("ruby\x1b]0;title\x07 \u{9b}31m \u{202e}cba\r"),
            r"ruby\u{1b}]0;title\u{7} \u{9b}31m \u{202e}cba\u{d}"
        );
        let printable = "/home/ü/ruby 3.3\n\twith `backticks`";
        assert!(matches!(
            sanitize_for_terminal(printable),
            Cow::Borrowed(s) if s == printable
        ));
    }

    #[test]
    fn test_cooldown_sentinel_never_writes_through_a_planted_symlink() {
        // Sentinels are written into a `.shadowenv.d` that isn't trusted yet.
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        fs::write(&victim, "precious").unwrap();
        let sentinel = err_file(dir.path(), 0, 4242).unwrap();
        std::os::unix::fs::symlink(&victim, &sentinel).unwrap();

        assert!(!check_cooldown_sentinel(&sentinel, cooldown()).unwrap());
        create_cooldown_sentinel(sentinel.clone()).unwrap();
        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
        assert!(sentinel.symlink_metadata().unwrap().is_file());
        assert!(check_cooldown_sentinel(&sentinel, cooldown()).unwrap());
    }
}
