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
    use std::path::PathBuf;

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
        // Create test data
        let test_path = PathBuf::from("/test/path/shadowenv");
        let test_script = b"#!/bin/sh\nPATH=@SELF@\n@HOOKBOOK@\n";
        
        // Redirect stdout to a Vec
        let mut output = Vec::new();
        {
            with_captured_stdout(&mut output, || {
                assert_eq!(print_script(test_path.clone(), test_script), 0);
            });
        }
        
        // Convert captured output to string
        let output_str = String::from_utf8(output).unwrap();
        
        // Verify substitutions
        assert!(output_str.contains("PATH=/test/path/shadowenv"));
        assert!(!output_str.contains("@SELF@"));
        assert!(!output_str.contains("@HOOKBOOK@"));
    }

    use std::io::{self, Write};

    // Helper function to capture stdout during tests
    #[cfg(test)]
    fn with_captured_stdout<F>(buf: &mut Vec<u8>, f: F)
    where F: FnOnce() {
        let mut stdout = io::stdout();
        let mut handle = io::BufWriter::new(buf);
        {
            let _lock = stdout.lock();
            f();
            handle.flush().unwrap();
        }
    }
}
