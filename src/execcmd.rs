use crate::hook;
use anyhow::Error;
use std::path::PathBuf;

/// Execute the provided command (argv) after loading the environment from the current directory
pub fn run(pathbuf: PathBuf, shadowenv_data: String, argv: Vec<String>) -> Result<(), Error> {
    if let Some(shadowenv) = hook::load_env(pathbuf, shadowenv_data, true)? {
        hook::mutate_own_env(&shadowenv)?;
    }

    // exec only returns if it was unable to start the new process, and it's always an error.
    let err = exec::Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(err.into())
}
