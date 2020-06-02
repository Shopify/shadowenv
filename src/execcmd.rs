use crate::hook;
use failure::Error;
use std::path::PathBuf;
use std::vec::Vec;

/// Execute the provided command (argv) after loading the environment from the current directory
pub fn run(pathbuf: PathBuf, shadowenv_data: String, argv: Vec<&str>) -> Result<(), Error> {
    match hook::load_env(pathbuf, shadowenv_data, true)? {
        Some((shadowenv, _)) => {
            hook::mutate_own_env(&shadowenv)?;
        }
        None => (),
    }

    // exec only returns if it was unable to start the new process, and it's always an error.
    let err = exec::Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(err.into())
}
