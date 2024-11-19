use crate::cli::InitCmd::{self, *};
use std::path::PathBuf;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(cmd: InitCmd) {
    let pb = std::env::current_exe().unwrap(); // this would be... an unusual failure.
    match cmd {
        Bash => print_script(pb, include_bytes!("../sh/shadowenv.bash.in")),
        Zsh => print_script(pb, include_bytes!("../sh/shadowenv.zsh.in")),
        Fish => print_script(pb, include_bytes!("../sh/shadowenv.fish.in")),
    };
}

fn print_script(selfpath: PathBuf, bytes: &[u8]) -> i32 {
    let hookbook = String::from_utf8_lossy(include_bytes!("../sh/hookbook.sh"));
    let script = String::from_utf8_lossy(bytes);
    let script = script.replace("@SELF@", selfpath.into_os_string().to_str().unwrap());
    let script = script.replace("@HOOKBOOK@", &hookbook);
    println!("{}", script);
    0
}
