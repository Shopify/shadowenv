use crate::shadowenv::Shadowenv;

pub fn run() {
    let data = Shadowenv::from_env();
    if !data.is_empty() && !data.starts_with("00000000") {
        print!("\x1b[38;5;245m░\x1b[0m");
    }
}
