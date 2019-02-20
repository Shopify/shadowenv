use regex::Regex;

// "shadowenv" in a gradient of lighter to darker grays. Looks good on dark backgrounds and ok on
// light backgrounds.
const SHADOWENV: &'static str = concat!(
    "\x1b[38;5;249ms\x1b[38;5;248mh\x1b[38;5;247ma\x1b[38;5;246md\x1b[38;5;245mo",
    "\x1b[38;5;244mw\x1b[38;5;243me\x1b[38;5;242mn\x1b[38;5;241mv\x1b[38;5;240m",
);

pub fn handle_hook_error(err: failure::Error) -> i32 {
    let err = backticks_to_bright_green(err);
    eprintln!("{} \x1b[1;31mfailure: {}\x1b[0m", SHADOWENV, err);
    return 1;
}

pub fn print_activation(activated: bool) {
    let word = match activated {
        true => "activated",
        false => "deactivated",
    };
    eprintln!("\x1b[1;34m{} {}.\x1b[0m", word, SHADOWENV);
}

fn backticks_to_bright_green(err: failure::Error) -> String {
    let re = Regex::new(r"`(.*?)`").unwrap();
    // this is almost certainly not the best way to do this, but this runs at most once per
    // execution so I only care so much.
    let before = format!("{}", err);
    re.replace_all(before.as_ref(), "\x1b[1;32m$1\x1b[1;31m")
        .to_string()
}
