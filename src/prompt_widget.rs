pub fn run(shadowenv_data: String) -> i32 {
    if !shadowenv_data.is_empty() && !shadowenv_data.starts_with("00000000") {
        print!("\x1b[38;5;245m░\x1b[0m");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_with_empty_data() {
        assert_eq!(run("".to_string()), 0);
    }

    #[test]
    fn test_run_with_zero_hash() {
        assert_eq!(run("00000000:{}".to_string()), 0);
    }

    #[test]
    fn test_run_with_nonzero_hash() {
        assert_eq!(run("12345678:{}".to_string()), 0);
    }

    #[test]
    fn test_run_with_malformed_data() {
        assert_eq!(run("invalid:data".to_string()), 0);
        assert_eq!(run(":".to_string()), 0);
        assert_eq!(run("12345678:".to_string()), 0);
    }

    #[test]
    fn test_run_with_unicode_hash() {
        assert_eq!(run("🦀rust:{}".to_string()), 0);
    }
}
