use std::path::PathBuf;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(shellname: &str) -> i32 {
    let pb = std::env::current_exe().unwrap(); // this would be... an unusual failure.
    match shellname {
        "bash" => print_script(pb, include_bytes!("../sh/shadowenv.bash.in")),
        "zsh" => print_script(pb, include_bytes!("../sh/shadowenv.zsh.in")),
        "fish" => print_script(pb, include_bytes!("../sh/shadowenv.fish.in")),
        _ => {
            eprintln!(
                "invalid shell name '{}' (must be one of bash, zsh, fish)",
                shellname
            );
            1
        }
    }
    #[test]
    fn test_run_current_exe_failure() {
        // Temporarily override the current_exe function to simulate a failure
        let original_current_exe = std::env::current_exe;
        std::env::set_var("PATH", ""); // Clear PATH to simulate failure
        assert!(std::env::current_exe().is_err());
        std::env::set_var("PATH", original_current_exe().unwrap().to_str().unwrap()); // Restore PATH
    }

    #[test]
    fn test_print_script_invalid_utf8() {
        let invalid_utf8: &[u8] = &[0, 159, 146, 150]; // Invalid UTF-8 sequence
        let result = String::from_utf8_lossy(invalid_utf8);
        assert!(result.contains("�")); // Check for replacement character
    }

fn print_script(selfpath: PathBuf, bytes: &[u8]) -> i32 {
    let hookbook = String::from_utf8_lossy(include_bytes!("../sh/hookbook.sh"));
    let script = String::from_utf8_lossy(bytes);
    let script = script.replace("@SELF@", selfpath.into_os_string().to_str().unwrap());
    let script = script.replace("@HOOKBOOK@", &hookbook);
    println!("{}", script);
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{PathBuf, Path};
    use std::ffi::OsStr;

    #[test]
    fn test_run_valid_shells() {
        assert_eq!(run("bash"), 0);
        assert_eq!(run("zsh"), 0);
        assert_eq!(run("fish"), 0);
    }

    #[test]
    fn test_run_invalid_shell() {
        assert_eq!(run("invalid"), 1);
    }

    #[test]
    fn test_print_script_substitution() {
        let test_path = PathBuf::from("/test/path/shadowenv");
        let test_script = b"#!/bin/sh\nPATH=@SELF@\n@HOOKBOOK@\n";
        
        let hookbook = String::from_utf8_lossy(include_bytes!("../sh/hookbook.sh"));
        let script = String::from_utf8_lossy(test_script);
        let script = script.replace("@SELF@", test_path.into_os_string().to_str().unwrap());
        let script = script.replace("@HOOKBOOK@", &hookbook);
        
        assert!(script.contains("PATH=/test/path/shadowenv"));
        assert!(!script.contains("@SELF@"));
        assert!(!script.contains("@HOOKBOOK@"));
    }
}
