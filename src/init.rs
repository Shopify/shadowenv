use std::path::PathBuf;
use std::fs;

/// print a script that can be sourced into the provided shell, and sets up the shadowenv shell
/// hooks.
pub fn run(selfpath: &str, shellname: &str) -> i32 {
    let pb = PathBuf::from(selfpath);
    let pb = fs::canonicalize(pb).unwrap(); // this would be... an unusual failure.
    match shellname.as_ref() {
        "bash" => print_script(pb, include_bytes!("../sh/shadowenv.bash")),
        "zsh" => print_script(pb, include_bytes!("../sh/shadowenv.zsh")),
        "fish" => print_script(pb, include_bytes!("../sh/shadowenv.fish")),
        _ => {
            eprintln!("invalid shell name '{}' (must be one of bash, zsh, fish)", shellname);
            1
        }
    }
}

fn print_script(selfpath: PathBuf, bytes: &[u8]) -> i32 {
    let script = String::from_utf8_lossy(bytes);
    let script = script.replace("{{self}}", selfpath.into_os_string().to_str().unwrap());
    println!("{}", script);
    0
}
