use crate::{features::Feature, loader, trust};
use anyhow::{anyhow, Error};
use regex::Regex;
use std::{
    collections::HashSet,
    env,
    fs::{self, OpenOptions},
    io::IsTerminal,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

// "shadowenv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const SHADOWENV: &str = concat!(
    "\x1b[38;5;249m░",
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
    1
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
                .map(|s| s.to_string())
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
    let before = format!("{}", err);
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
fn check_cooldown_sentinel(path: &Path, timeout: Duration) -> Result<bool, Error> {
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

    std::io::stderr().is_terminal() && configured_to_print
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::env;
    use std::fs::File;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn test_handle_hook_error() {
        let err = anyhow!("test error");
        let result = handle_hook_error(err, 1234, true);
        assert_eq!(result, 1);

        let err = anyhow!("test error");
        let result = handle_hook_error(err, 1234, false);
        assert_eq!(result, 1);
    }

    #[test]
    fn test_should_print_activation() {
        // Test with SHADOWENV_SILENT unset
        env::remove_var("SHADOWENV_SILENT");
        // Note: Result depends on whether stderr is a terminal
        let default_result = should_print_activation();

        // Test with SHADOWENV_SILENT=false
        env::set_var("SHADOWENV_SILENT", "false");
        assert_eq!(should_print_activation(), default_result);

        // Test with SHADOWENV_SILENT=true
        env::set_var("SHADOWENV_SILENT", "true");
        assert_eq!(should_print_activation(), false);

        // Cleanup
        env::remove_var("SHADOWENV_SILENT");
    }

    #[test]
    fn test_print_activation_to_tty() {
        let mut current_dirs = HashSet::new();
        let mut prev_dirs = HashSet::new();
        let mut features = HashSet::new();

        current_dirs.insert(PathBuf::from("/test/current"));
        prev_dirs.insert(PathBuf::from("/test/prev"));
        features.insert(Feature::new("test".to_string(), Some("1.0".to_string())));

        // This will only print if stderr is a terminal
        print_activation_to_tty(current_dirs, prev_dirs, features);
        // We can't easily assert the output since it depends on stderr being a terminal
        // But we can verify it doesn't panic
    }

    #[test]
    fn test_backticks_to_bright_green() {
        let err = anyhow!("Error in `command` and another `thing`");
        let result = backticks_to_bright_green(err);
        assert!(result.contains("\x1b[1;32mcommand\x1b[1;31m"));
        assert!(result.contains("\x1b[1;32mthing\x1b[1;31m"));
    }

    #[test]
    fn test_dir_diff_empty() {
        let added = HashSet::new();
        let removed = HashSet::new();
        assert_eq!(dir_diff(added, removed), None);
    }

    #[test]
    fn test_dir_diff_additions() {
        let mut added = HashSet::new();
        added.insert(PathBuf::from("/test/path1"));
        added.insert(PathBuf::from("/test/path2"));
        let removed = HashSet::new();

        let result = dir_diff(added, removed).unwrap();
        assert!(result.contains("\x1b[0;32m++"));
        assert!(!result.contains("|"));
        assert!(!result.contains("-"));
    }

    #[test]
    fn test_dir_diff_removals() {
        let added = HashSet::new();
        let mut removed = HashSet::new();
        removed.insert(PathBuf::from("/test/path1"));
        removed.insert(PathBuf::from("/test/path2"));

        let result = dir_diff(added, removed).unwrap();
        assert!(!result.contains("+"));
        assert!(!result.contains("|"));
        assert!(result.contains("\x1b[0;31m--"));
    }

    #[test]
    fn test_dir_diff_both() {
        let mut added = HashSet::new();
        added.insert(PathBuf::from("/test/path1"));
        let mut removed = HashSet::new();
        removed.insert(PathBuf::from("/test/path2"));

        let result = dir_diff(added, removed).unwrap();
        assert!(result.contains("\x1b[0;32m+"));
        assert!(result.contains("\x1b[38;5;240m|"));
        assert!(result.contains("\x1b[0;31m-"));
    }

    #[test]
    fn test_cooldown_functionality() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Test creating cooldown sentinel
        let sentinel_path = root.join(".error-0-12345");
        assert!(create_cooldown_sentinel(sentinel_path.clone()).is_ok());
        assert!(sentinel_path.exists());

        // Test checking cooldown - should be active
        assert!(check_cooldown_sentinel(&sentinel_path, cooldown()).unwrap());

        // Test with expired cooldown
        let old_sentinel_path = root.join(".error-1-12345");
        File::create(&old_sentinel_path).unwrap();

        // Set old modification time
        let old_time = SystemTime::now() - Duration::from_secs(COOLDOWN_SECONDS + 1);
        filetime::set_file_mtime(
            &old_sentinel_path,
            filetime::FileTime::from_system_time(old_time),
        )
        .unwrap();

        assert!(!check_cooldown_sentinel(&old_sentinel_path, cooldown()).unwrap());
    }

    #[test]
    fn test_clean_up_stale_errors() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        // Create some error files
        let fresh_error = root.join(".error-0-12345");
        let stale_error = root.join(".error-1-12345");
        let non_error = root.join("not-an-error");

        File::create(&fresh_error).unwrap();
        File::create(&stale_error).unwrap();
        File::create(&non_error).unwrap();

        // Make stale_error old
        let old_time = SystemTime::now() - Duration::from_secs(301); // Just over 5 minutes
        filetime::set_file_mtime(&stale_error, filetime::FileTime::from_system_time(old_time))
            .unwrap();

        // Clean up stale errors
        clean_up_stale_errors(&root, Duration::from_secs(300)).unwrap();

        // Verify results
        assert!(fresh_error.exists());
        assert!(!stale_error.exists());
        assert!(non_error.exists());
    }

    #[test]
    fn test_err_file_generation() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();

        let err_path = err_file(&root, 0, 12345).unwrap();
        assert_eq!(err_path, root.join(".error-0-12345"));
    }

    #[test]
    fn test_check_and_trigger_cooldown() {
        let temp_dir = tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        fs::create_dir(root.join(".shadowenv.d")).unwrap();

        env::set_current_dir(&root).unwrap();

        // Test with non-cooldown error
        let regular_error = anyhow!("regular error");
        assert!(check_and_trigger_cooldown(&regular_error, 12345).is_err());

        // Test with cooldown error (NotTrusted)
        let trust_error = anyhow::Error::from(trust::NotTrusted {
            untrusted_directories: vec!["test".to_string()],
        });

        // First trigger should return false (not on cooldown)
        assert!(!check_and_trigger_cooldown(&trust_error, 12345).unwrap());

        // Second trigger should return true (on cooldown)
        assert!(check_and_trigger_cooldown(&trust_error, 12345).unwrap());
    }
}
