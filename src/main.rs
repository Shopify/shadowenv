mod cli;
mod diff;
mod execcmd;
mod features;
mod hash;
mod hook;
mod init;
mod lang;
mod loader;
mod output;
mod prompt_widget;
mod shadowenv;
mod trust;
mod undo;

use crate::shadowenv::Shadowenv;
use anyhow::{anyhow, Error};
use clap::Parser;
use cli::ExecCmd;
use std::{env, iter, path::PathBuf, process};

fn main() {
    use cli::ShadowenvApp::*;

    let result = match cli::ShadowenvApp::parse() {
        Diff(cmd) => Ok(diff::run(cmd)),
        Exec(cmd) => run_exec(cmd),
        Hook(cmd) => hook::run(cmd),
        Init(cmd) => Ok(init::run(cmd)),
        Trust(_) => trust::run(),
        PromptWidget(_) => Ok(prompt_widget::run()),
    };

    if let Err(err) = result {
        if err.to_string() != "" {
            eprintln!("{}", err);
        }

        process::exit(1);
    }
}

fn run_exec(cmd: ExecCmd) -> Result<(), Error> {
    let data = Shadowenv::from_env();
    let pathbuf = cmd
        .dir
        .map(|d| PathBuf::from(d))
        .unwrap_or(get_current_dir_or_exit());

    let argv = iter::once(cmd.cmd).chain(cmd.cmd_argv);
    execcmd::run(pathbuf, data, argv.collect())
}

fn get_current_dir_or_exit() -> PathBuf {
    match env::current_dir() {
        Ok(dir) => dir,
        Err(_) => process::exit(0), // If the current dir was deleted, there's not much we can do. Just exit silently.
    }
}

fn unsafe_getppid() -> Result<u32, Error> {
    let ppid;
    unsafe { ppid = libc::getppid() }
    if ppid < 1 {
        return Err(anyhow!("failed to get ppid"));
    }
    Ok(ppid as u32)
}
