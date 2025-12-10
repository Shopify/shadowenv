use crate::cli::InitCmd::{self, *};
use std::path::PathBuf;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(cmd: InitCmd) {
    let pb = std::env::current_exe().unwrap(); // this would be... an unusual failure.
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
        Nushell => print_script(
            pb,
            include_bytes!("../sh/shadowenv.nushell.in"),
            true, // Nushell doesn't use hookbook
        ),
    };
}

fn print_script(selfpath: PathBuf, bytes: &[u8], no_hookbook: bool) -> i32 {
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
    0
}
