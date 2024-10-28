pub fn run(shadowenv_data: String) -> i32 {
    if !shadowenv_data.is_empty() && !shadowenv_data.starts_with("00000000") {
        print!("\x1b[38;5;245m░\x1b[0m");
    }
    0
}
