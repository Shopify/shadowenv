use crate::{cli::ExecCmd, get_current_dir_or_exit, hook, shadowenv::Shadowenv};
use anyhow::Error;
use std::{iter, path::PathBuf};

/// Execute the provided command (argv) after loading the environment from the current directory
pub fn run(cmd: ExecCmd) -> Result<(), Error> {
    let data = Shadowenv::from_env();
    let pathbuf = cmd
        .dir
        .map(PathBuf::from)
        .unwrap_or(get_current_dir_or_exit());

    if let Some(shadowenv) = hook::load_env(pathbuf, data, true, false)? {
        hook::mutate_own_env(&shadowenv)?;
    }

    let argv = if let Some(argv0) = cmd.cmd_argv0 {
        iter::once(argv0).chain(cmd.cmd_argv).collect::<Vec<_>>()
    } else if !cmd.cmd_argv.is_empty() {
        cmd.cmd_argv
    } else {
        return Err(anyhow::anyhow!("no command provided"));
    };

    // exec only returns if it was unable to start the new process, and it's always an error.
    let err = exec::Command::new(&argv[0]).args(&argv[1..]).exec();
    Err(err.into())
}
