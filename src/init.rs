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
    use std::io::{self, Write};

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
        
        // Capture stdout
        let mut output = Vec::new();
        {
            let mut old_stdout = io::stdout();
            std::io::set_print!({
                let mut w = &mut output;
                move |s| w.write_all(s.as_bytes()).map(|_| ())
            });
            
            assert_eq!(print_script(test_path.clone(), test_script), 0);
        }
        
        // Convert captured output to string
        let output_str = String::from_utf8(output).unwrap();
        
        // Verify substitutions
        assert!(output_str.contains("/test/path/shadowenv"));
        assert!(!output_str.contains("@SELF@"));
        assert!(!output_str.contains("@HOOKBOOK@"));
    }
}
