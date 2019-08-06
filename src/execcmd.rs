use failure::Error;
use std::vec::Vec;
use crate::hook;

/// Execute the provided command (argv) after loading the environment from the current directory
/// (with respect to the optional $__shadowenv_data (`data`)).
pub fn run(data: Option<&str>, argv: Vec<&str>) -> Result<(), Error> {
    let shadowenv_data = data.unwrap_or("");
    match hook::load_env(shadowenv_data)? {
        Some((shadowenv, _)) => { hook::mutate_own_env(&shadowenv)?; },
        None                 => (),
    }

    // if it returns, it's guaranteed to be an error.
    let err = exec::Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(err.into())
}
